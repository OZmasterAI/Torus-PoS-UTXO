// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable2Step.sol";
import "@openzeppelin/contracts/utils/Pausable.sol";

/// @title IVerifier — swappable verification interface
/// @notice Start with ThresholdVerifier, upgrade to SP1Verifier (ZK) later.
/// The WrappedTRS contract only calls this interface, so the backend can
/// change from threshold signatures to ZK proofs without redeploying the token.
interface IVerifier {
    /// @param proof       Encoded proof data (threshold sig or ZK proof)
    /// @param blockHash   Torus-core block hash containing the deposit
    /// @param txHash      Deposit transaction hash
    /// @param amount      Deposit amount in TRS base units
    /// @param recipient   Ethereum address to receive wrapped tokens
    /// @return valid      True if the proof is valid
    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view returns (bool valid);
}

/// @title WrappedTRS — ERC-20 wrapped Torus token with swappable verifier
/// @notice Deposits are verified by an IVerifier implementation.
///         Phase A: ThresholdVerifier (t-of-n multisig)
///         Phase B: SP1Verifier (ZK proof of PoS consensus)
contract WrappedTRS is ERC20, Ownable2Step, Pausable {
    IVerifier public verifier;
    uint256 public minDeposit;
    uint256 public maxDeposit;

    mapping(bytes32 => bool) public processedDeposits;
    mapping(bytes32 => bool) public processedWithdrawals;

    event VerifierUpdated(address indexed oldVerifier, address indexed newVerifier);
    event Deposited(bytes32 indexed txHash, address indexed recipient, uint256 amount);
    event WithdrawalRequested(bytes32 indexed withdrawalId, address indexed sender, uint256 amount, bytes torusAddress);

    error InvalidVerifier();
    error InvalidProof();
    error DepositAlreadyProcessed();
    error AmountBelowMinimum();
    error AmountAboveMaximum();
    error ZeroAmount();
    error InvalidRecipient();

    constructor(
        address _verifier,
        uint256 _minDeposit,
        uint256 _maxDeposit
    ) ERC20("Wrapped TRS", "wTRS") Ownable(msg.sender) {
        if (_verifier == address(0)) revert InvalidVerifier();
        verifier = IVerifier(_verifier);
        minDeposit = _minDeposit;
        maxDeposit = _maxDeposit;
    }

    /// @notice Mint wrapped tokens after verifying a torus-core deposit.
    /// @param proof     Proof data (format depends on current verifier)
    /// @param blockHash Block hash containing the deposit transaction
    /// @param txHash    Deposit transaction hash (used as unique deposit ID)
    /// @param amount    Amount of TRS deposited (in base units, 8 decimals)
    /// @param recipient Ethereum address to receive wTRS
    function mint(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external whenNotPaused {
        if (amount == 0) revert ZeroAmount();
        if (amount < minDeposit) revert AmountBelowMinimum();
        if (maxDeposit > 0 && amount > maxDeposit) revert AmountAboveMaximum();
        if (recipient == address(0)) revert InvalidRecipient();
        if (processedDeposits[txHash]) revert DepositAlreadyProcessed();

        bool valid = verifier.verifyDeposit(proof, blockHash, txHash, amount, recipient);
        if (!valid) revert InvalidProof();

        processedDeposits[txHash] = true;
        _mint(recipient, amount);

        emit Deposited(txHash, recipient, amount);
    }

    /// @notice Burn wTRS from caller's balance. Used by BridgeController
    ///         to destroy tokens after a confirmed withdrawal.
    /// @param amount Amount of wTRS to burn
    function burn(uint256 amount) external {
        _burn(msg.sender, amount);
    }

    function withdraw(uint256 amount, bytes calldata torusAddress) external whenNotPaused {
        if (amount == 0) revert ZeroAmount();

        bytes32 withdrawalId = keccak256(
            abi.encodePacked(msg.sender, amount, torusAddress, block.number)
        );

        _burn(msg.sender, amount);
        processedWithdrawals[withdrawalId] = true;

        emit WithdrawalRequested(withdrawalId, msg.sender, amount, torusAddress);
    }

    // --- Admin functions ---

    function setVerifier(address _newVerifier) external onlyOwner {
        if (_newVerifier == address(0)) revert InvalidVerifier();
        address old = address(verifier);
        verifier = IVerifier(_newVerifier);
        emit VerifierUpdated(old, _newVerifier);
    }

    function setDepositLimits(uint256 _min, uint256 _max) external onlyOwner {
        minDeposit = _min;
        maxDeposit = _max;
    }

    function pause() external onlyOwner {
        _pause();
    }

    function unpause() external onlyOwner {
        _unpause();
    }

    function decimals() public pure override returns (uint8) {
        return 8; // Match torus-core's COIN = 1e8
    }
}

/// @title ThresholdVerifier — Phase A: t-of-n threshold signature verification
/// @notice Simple multisig verification. Signers attest that a deposit occurred
///         on torus-core. Replaced by SP1Verifier in Phase B.
contract ThresholdVerifier is IVerifier {
    address[] public signers;
    uint256 public threshold;

    bytes32 public immutable DOMAIN_SEPARATOR;
    bytes32 private constant DEPOSIT_TYPEHASH =
        keccak256("VerifyDeposit(bytes32 blockHash,bytes32 txHash,uint256 amount,address recipient)");

    constructor(address[] memory _signers, uint256 _threshold) {
        require(_threshold > 0 && _threshold <= _signers.length, "invalid threshold");
        for (uint256 i = 0; i < _signers.length; i++) {
            for (uint256 j = i + 1; j < _signers.length; j++) {
                require(_signers[i] != _signers[j], "duplicate signer");
            }
        }
        signers = _signers;
        threshold = _threshold;
        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("TorusBridgeVerifier"),
                keccak256("1"),
                block.chainid,
                address(this)
            )
        );
    }

    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view override returns (bool) {
        bytes32 structHash = keccak256(
            abi.encode(DEPOSIT_TYPEHASH, blockHash, txHash, amount, recipient)
        );
        bytes32 digest = keccak256(
            abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR, structHash)
        );

        uint256 sigCount = proof.length / 65;
        if (sigCount < threshold) return false;

        uint256 validSigs = 0;
        uint256 seenBitmap = 0;
        for (uint256 i = 0; i < sigCount; i++) {
            bytes memory sig = proof[i * 65:(i + 1) * 65];
            address recovered = recoverSigner(digest, sig);
            uint256 signerIdx = signerIndex(recovered);
            if (signerIdx != type(uint256).max) {
                uint256 bit = 1 << signerIdx;
                if (seenBitmap & bit == 0) {
                    seenBitmap |= bit;
                    validSigs++;
                }
            }
        }

        return validSigs >= threshold;
    }

    function signerIndex(address addr) internal view returns (uint256) {
        for (uint256 i = 0; i < signers.length; i++) {
            if (signers[i] == addr) return i;
        }
        return type(uint256).max;
    }

    function recoverSigner(bytes32 digest, bytes memory sig) internal pure returns (address) {
        (bytes32 r, bytes32 s, uint8 v) = splitSignature(sig);
        return ecrecover(digest, v, r, s);
    }

    function splitSignature(bytes memory sig) internal pure returns (bytes32 r, bytes32 s, uint8 v) {
        require(sig.length == 65, "invalid sig length");
        assembly {
            r := mload(add(sig, 32))
            s := mload(add(sig, 64))
            v := byte(0, mload(add(sig, 96)))
        }
    }
}

/// @title ISP1VerifierGateway — SP1's on-chain Groth16 verification gateway
interface ISP1VerifierGateway {
    function verifyProof(
        bytes32 programVKey,
        bytes calldata publicValues,
        bytes calldata proofBytes
    ) external view;
}

/// @title SP1Verifier — Phase B: ZK proof of torus-core PoS consensus
///         Proof layout: [publicValues (192 bytes) | groth16Proof (variable)]
///         publicValues (6 × 32 bytes, big-endian):
///           [0:32]    mode (0=deposit, 1=withdrawal)
///           [32:64]   blockHash
///           [64:96]   kernelHash (PoS proof-of-stake, verified inside ZK)
///           [96:128]  txHash
///           [128:160] amount (uint256)
///           [160:192] recipient (address, left-padded to 32 bytes)
contract SP1Verifier is IVerifier {
    ISP1VerifierGateway public immutable gateway;
    bytes32 public immutable programVKey;

    error ProofTooShort();
    error WrongProofMode();
    error BlockHashMismatch();
    error TxHashMismatch();
    error AmountMismatch();
    error RecipientMismatch();

    constructor(address _gateway, bytes32 _vkey) {
        gateway = ISP1VerifierGateway(_gateway);
        programVKey = _vkey;
    }

    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view override returns (bool) {
        if (proof.length <= 192) revert ProofTooShort();

        uint256 pMode;
        bytes32 pBlockHash;
        bytes32 pTxHash;
        uint256 pAmount;
        uint256 recipientWord;

        assembly {
            pMode := calldataload(proof.offset)
            pBlockHash := calldataload(add(proof.offset, 32))
            pTxHash := calldataload(add(proof.offset, 96))
            pAmount := calldataload(add(proof.offset, 128))
            recipientWord := calldataload(add(proof.offset, 160))
        }

        if (pMode != 0) revert WrongProofMode();
        if (pBlockHash != blockHash) revert BlockHashMismatch();
        if (pTxHash != txHash) revert TxHashMismatch();
        if (pAmount != amount) revert AmountMismatch();
        if (address(uint160(recipientWord)) != recipient) revert RecipientMismatch();

        gateway.verifyProof(programVKey, proof[:192], proof[192:]);

        return true;
    }

    function verifyWithdrawal(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address requester
    ) external view returns (bool) {
        if (proof.length <= 192) revert ProofTooShort();

        uint256 pMode;
        bytes32 pBlockHash;
        bytes32 pTxHash;
        uint256 pAmount;

        assembly {
            pMode := calldataload(proof.offset)
            pBlockHash := calldataload(add(proof.offset, 32))
            pTxHash := calldataload(add(proof.offset, 96))
            pAmount := calldataload(add(proof.offset, 128))
        }

        if (pMode != 1) revert WrongProofMode();
        if (pBlockHash != blockHash) revert BlockHashMismatch();
        if (pTxHash != txHash) revert TxHashMismatch();
        if (pAmount != amount) revert AmountMismatch();

        gateway.verifyProof(programVKey, proof[:192], proof[192:]);

        return true;
    }
}
