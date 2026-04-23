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

    constructor(address[] memory _signers, uint256 _threshold) {
        require(_threshold > 0 && _threshold <= _signers.length, "invalid threshold");
        signers = _signers;
        threshold = _threshold;
    }

    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view override returns (bool) {
        bytes32 message = keccak256(
            abi.encodePacked(blockHash, txHash, amount, recipient)
        );

        // Decode proof as concatenated 65-byte ECDSA signatures
        uint256 sigCount = proof.length / 65;
        if (sigCount < threshold) return false;

        uint256 validSigs = 0;
        for (uint256 i = 0; i < sigCount; i++) {
            bytes memory sig = proof[i * 65:(i + 1) * 65];
            address recovered = recoverSigner(message, sig);
            if (isValidSigner(recovered)) {
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

/// @title SP1Verifier — Phase B: ZK proof verification (placeholder)
/// @notice Verifies SP1-generated Groth16 proofs of torus-core PoS consensus.
///         Deploy this and call WrappedTRS.setVerifier() to upgrade from threshold.
///
/// Implementation notes:
///   - Use the SP1 Solidity verifier generated by `cargo prove build --groth16`
///   - The proof encodes: block header chain validity, PoS kernel hash check,
///     merkle inclusion of deposit tx
///   - Public outputs: blockHash, txHash, amount, recipient
///   - Verification cost: ~270k gas (Groth16 on BN254)
contract SP1Verifier is IVerifier {
    // address public immutable sp1VerifierGateway;
    // bytes32 public immutable programVKey;

    // constructor(address _gateway, bytes32 _vkey) {
    //     sp1VerifierGateway = _gateway;
    //     programVKey = _vkey;
    // }

    function verifyDeposit(
        bytes calldata /* proof */,
        bytes32 /* blockHash */,
        bytes32 /* txHash */,
        uint256 /* amount */,
        address /* recipient */
    ) external pure override returns (bool) {
        // Phase B implementation:
        //
        // 1. Decode the SP1 Groth16 proof from `proof` bytes
        // 2. Extract public outputs (blockHash, txHash, amount, recipient)
        // 3. Verify public outputs match function parameters
        // 4. Call SP1VerifierGateway.verifyProof(programVKey, publicValues, proof)
        // 5. Return true if verification passes
        //
        // Estimated gas: ~270k (Groth16) + ~50k (ERC-20 mint) = ~320k total

        revert("SP1Verifier: not yet implemented");
    }
}
