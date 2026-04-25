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

    /// @notice Burn wrapped tokens and request withdrawal to torus-core.
    /// @param amount       Amount of wTRS to burn
    /// @param torusAddress Torus-core destination address (raw bytes)
    error WithdrawalAlreadyProcessed();

    function withdraw(uint256 amount, bytes calldata torusAddress) external whenNotPaused {
        if (amount == 0) revert ZeroAmount();

        bytes32 withdrawalId = keccak256(
            abi.encodePacked(msg.sender, amount, torusAddress, block.number)
        );

        if (processedWithdrawals[withdrawalId]) revert WithdrawalAlreadyProcessed();
        processedWithdrawals[withdrawalId] = true;
        _burn(msg.sender, amount);

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
    bytes32 private constant DEPOSIT_TYPEHASH = keccak256(
        "VerifyDeposit(bytes32 blockHash,bytes32 txHash,uint256 amount,address recipient)"
    );

    constructor(address[] memory _signers, uint256 _threshold) {
        require(_threshold > 0 && _threshold <= _signers.length, "invalid threshold");
        signers = _signers;
        threshold = _threshold;
        DOMAIN_SEPARATOR = keccak256(
            abi.encode(
                keccak256("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
                keccak256("ThresholdVerifier"),
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
        bytes32 message = keccak256(
            abi.encodePacked("\x19\x01", DOMAIN_SEPARATOR, structHash)
        );

        // Decode proof as concatenated 65-byte ECDSA signatures
        uint256 sigCount = proof.length / 65;
        if (sigCount < threshold) return false;

        uint256 validSigs = 0;
        address[] memory seen = new address[](sigCount);
        uint256 seenCount = 0;
        for (uint256 i = 0; i < sigCount; i++) {
            bytes memory sig = proof[i * 65:(i + 1) * 65];
            address recovered = recoverSigner(message, sig);
            if (recovered == address(0)) continue;

            bool duplicate = false;
            for (uint256 j = 0; j < seenCount; j++) {
                if (seen[j] == recovered) { duplicate = true; break; }
            }
            if (duplicate) continue;

            if (isValidSigner(recovered)) {
                seen[seenCount++] = recovered;
                validSigs++;
            }
        }

        return validSigs >= threshold;
    }

    function isValidSigner(address addr) internal view returns (bool) {
        for (uint256 i = 0; i < signers.length; i++) {
            if (signers[i] == addr) return true;
        }
        return false;
    }

    function recoverSigner(bytes32 message, bytes memory sig) internal pure returns (address) {
        bytes32 ethMessage = keccak256(
            abi.encodePacked("\x19Ethereum Signed Message:\n32", message)
        );
        (bytes32 r, bytes32 s, uint8 v) = splitSignature(sig);
        return ecrecover(ethMessage, v, r, s);
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
/// @notice Verifies Groth16 proofs generated by the SP1 bridge circuit.
///         The circuit proves: (1) block header hash, (2) PoS kernel hash meets
///         target, (3) deposit tx Merkle inclusion in the block.
///         Deploy this and call WrappedTRS.setVerifier() to upgrade from threshold.
///
///         Proof layout passed to verifyDeposit():
///           [publicValues (160 bytes) | groth16Proof (variable)]
///         publicValues (5 × 32 bytes, big-endian):
///           [0:32]    blockHash
///           [32:64]   kernelHash (PoS proof-of-stake, verified inside ZK)
///           [64:96]   txHash
///           [96:128]  amount (uint256)
///           [128:160] recipient (address, left-padded to 32 bytes)
///
///         Gas cost: ~270k (Groth16 pairing) + ~50k (ERC-20 mint) ≈ 320k total
contract SP1Verifier is IVerifier {
    ISP1VerifierGateway public immutable gateway;
    bytes32 public immutable programVKey;
    bytes32 public immutable posTargetHash;

    error ProofTooShort();
    error BlockHashMismatch();
    error KernelHashExceedsTarget();
    error TxHashMismatch();
    error AmountMismatch();
    error RecipientMismatch();

    constructor(address _gateway, bytes32 _vkey, bytes32 _posTarget) {
        gateway = ISP1VerifierGateway(_gateway);
        programVKey = _vkey;
        posTargetHash = _posTarget;
    }

    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view override returns (bool) {
        if (proof.length <= 160) revert ProofTooShort();

        bytes32 pBlockHash;
        bytes32 pKernelHash;
        bytes32 pTxHash;
        uint256 pAmount;
        uint256 recipientWord;

        assembly {
            pBlockHash := calldataload(proof.offset)
            pKernelHash := calldataload(add(proof.offset, 32))
            pTxHash := calldataload(add(proof.offset, 64))
            pAmount := calldataload(add(proof.offset, 96))
            recipientWord := calldataload(add(proof.offset, 128))
        }

        if (pBlockHash != blockHash) revert BlockHashMismatch();
        if (uint256(pKernelHash) > uint256(posTargetHash)) revert KernelHashExceedsTarget();
        if (pTxHash != txHash) revert TxHashMismatch();
        if (pAmount != amount) revert AmountMismatch();
        if (address(uint160(recipientWord)) != recipient) revert RecipientMismatch();

        gateway.verifyProof(programVKey, proof[:160], proof[160:]);

        return true;
    }
}
