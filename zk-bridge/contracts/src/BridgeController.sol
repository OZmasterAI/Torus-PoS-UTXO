// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title IVerifier — verification interface (same as WrappedTRS)
/// @notice Reuses the same verifyDeposit signature. For withdrawals the
///         verifier proves a torus-core transaction exists in a valid PoS
///         block with the correct amount and recipient.
interface IVerifier {
    function verifyDeposit(
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash,
        uint256 amount,
        address recipient
    ) external view returns (bool valid);
}

/// @title BridgeController — operator registry, withdrawal tracking, slashing
/// @notice Manages bridge operators who stake ETH, processes user withdrawals
///         from the EVM side back to torus-core, and slashes operators for
///         failing to fulfill withdrawals within the deadline.
contract BridgeController is ReentrancyGuard {
    using SafeERC20 for IERC20;

    // --- Types ---

    struct Operator {
        uint256 stake;
        uint256 unstakeInitiated;
        bool active;
    }

    struct WithdrawalRequest {
        address requester;
        uint256 amount;
        bytes20 torusAddress;
        uint256 requestedAt;
        uint256 deadline;
        bool completed;
    }

    // --- State ---

    IVerifier public immutable withdrawalVerifier;
    IERC20 public immutable token;
    uint256 public immutable minStake;
    uint256 public immutable withdrawalTimeout;
    uint256 public immutable unstakeCooldown;

    mapping(address => Operator) public operators;
    address[] public operatorList;

    mapping(bytes32 => WithdrawalRequest) public withdrawals;
    uint256 public withdrawalNonce;

    // --- Events ---

    event OperatorRegistered(address indexed operator, uint256 stake);
    event UnstakeInitiated(address indexed operator, uint256 timestamp);
    event UnstakeCompleted(address indexed operator, uint256 amount);
    event WithdrawalRequested(
        bytes32 indexed id,
        address requester,
        uint256 amount,
        bytes20 torusAddress,
        uint256 deadline
    );
    event WithdrawalCompleted(bytes32 indexed id, bytes32 torusTxHash);
    event OperatorSlashed(bytes32 indexed withdrawalId, uint256 totalSlashed);

    // --- Errors ---

    error InsufficientStake();
    error AlreadyRegistered();
    error NotOperator();
    error UnstakeNotInitiated();
    error CooldownNotElapsed();
    error OperatorStillActive();
    error ZeroAmount();
    error WithdrawalNotFound();
    error WithdrawalAlreadyCompleted();
    error InvalidProof();
    error DeadlineNotPassed();
    error TransferFailed();

    // --- Constructor ---

    constructor(
        IVerifier _withdrawalVerifier,
        IERC20 _token,
        uint256 _minStake,
        uint256 _withdrawalTimeout,
        uint256 _unstakeCooldown
    ) {
        withdrawalVerifier = _withdrawalVerifier;
        token = _token;
        minStake = _minStake;
        withdrawalTimeout = _withdrawalTimeout;
        unstakeCooldown = _unstakeCooldown;
    }

    // --- Operator Registry ---

    /// @notice Register as a bridge operator by staking ETH.
    ///         Must send at least minStake wei.
    function registerOperator() external payable {
        if (msg.value < minStake) revert InsufficientStake();
        if (operators[msg.sender].active) revert AlreadyRegistered();

        operators[msg.sender] = Operator({
            stake: msg.value,
            unstakeInitiated: 0,
            active: true
        });
        operatorList.push(msg.sender);

        emit OperatorRegistered(msg.sender, msg.value);
    }

    /// @notice Begin the unstake process. Operator remains active until
    ///         completeUnstake() is called after the cooldown period.
    function initiateUnstake() external {
        Operator storage op = operators[msg.sender];
        if (!op.active) revert NotOperator();

        op.unstakeInitiated = block.timestamp;

        emit UnstakeInitiated(msg.sender, block.timestamp);
    }

    /// @notice Complete unstaking after cooldown. Returns staked ETH and
    ///         deactivates the operator.
    function completeUnstake() external nonReentrant {
        Operator storage op = operators[msg.sender];
        if (!op.active) revert NotOperator();
        if (op.unstakeInitiated == 0) revert UnstakeNotInitiated();
        if (block.timestamp < op.unstakeInitiated + unstakeCooldown) {
            revert CooldownNotElapsed();
        }

        uint256 amount = op.stake;
        op.stake = 0;
        op.active = false;
        op.unstakeInitiated = 0;

        (bool sent, ) = msg.sender.call{value: amount}("");
        if (!sent) revert TransferFailed();

        emit UnstakeCompleted(msg.sender, amount);
    }

    // --- Withdrawal Flow ---

    /// @notice Request a withdrawal: transfers wTRS from the caller to this
    ///         contract (requires prior approval), records the request, and
    ///         starts the deadline clock.
    /// @param amount       Amount of wTRS to withdraw
    /// @param torusAddress Destination address on torus-core (20-byte hash160)
    function requestWithdrawal(
        uint256 amount,
        bytes20 torusAddress
    ) external nonReentrant {
        if (amount == 0) revert ZeroAmount();

        // Transfer wTRS from caller (requires approval)
        token.safeTransferFrom(msg.sender, address(this), amount);

        bytes32 id = keccak256(
            abi.encodePacked(
                msg.sender,
                amount,
                torusAddress,
                withdrawalNonce
            )
        );
        withdrawalNonce++;

        uint256 deadline = block.timestamp + withdrawalTimeout;

        withdrawals[id] = WithdrawalRequest({
            requester: msg.sender,
            amount: amount,
            torusAddress: torusAddress,
            requestedAt: block.timestamp,
            deadline: deadline,
            completed: false
        });

        emit WithdrawalRequested(id, msg.sender, amount, torusAddress, deadline);
    }

    /// @notice Confirm that a withdrawal was fulfilled on torus-core by
    ///         providing a ZK proof of the torus transaction.
    /// @param withdrawalId ID of the withdrawal request
    /// @param proof        ZK proof data (same format as deposit verifier)
    /// @param blockHash    Torus-core block hash containing the withdrawal tx
    /// @param txHash       Torus-core transaction hash of the withdrawal
    function confirmWithdrawal(
        bytes32 withdrawalId,
        bytes calldata proof,
        bytes32 blockHash,
        bytes32 txHash
    ) external nonReentrant {
        WithdrawalRequest storage w = withdrawals[withdrawalId];
        if (w.requester == address(0)) revert WithdrawalNotFound();
        if (w.completed) revert WithdrawalAlreadyCompleted();

        // Verify proof: the verifier checks that a torus-core transaction
        // with the correct amount exists in a valid PoS block.
        // We pass w.requester as the recipient for the proof check.
        bool valid = withdrawalVerifier.verifyDeposit(
            proof,
            blockHash,
            txHash,
            w.amount,
            w.requester
        );
        if (!valid) revert InvalidProof();

        w.completed = true;

        emit WithdrawalCompleted(withdrawalId, txHash);
    }

    /// @notice Slash all active operators if a withdrawal deadline has passed
    ///         without confirmation. Each active operator loses 50% of their
    ///         stake; the total slashed amount is sent to the requester.
    /// @param withdrawalId ID of the timed-out withdrawal request
    function slashForTimeout(bytes32 withdrawalId) external nonReentrant {
        WithdrawalRequest storage w = withdrawals[withdrawalId];
        if (w.requester == address(0)) revert WithdrawalNotFound();
        if (w.completed) revert WithdrawalAlreadyCompleted();
        if (block.timestamp < w.deadline) revert DeadlineNotPassed();

        w.completed = true; // Prevent re-slashing

        uint256 totalSlashed = 0;
        uint256 len = operatorList.length;
        for (uint256 i = 0; i < len; i++) {
            Operator storage op = operators[operatorList[i]];
            if (op.active && op.stake > 0) {
                uint256 slash = op.stake / 2;
                op.stake -= slash;
                totalSlashed += slash;
            }
        }

        if (totalSlashed > 0) {
            (bool sent, ) = w.requester.call{value: totalSlashed}("");
            if (!sent) revert TransferFailed();
        }

        emit OperatorSlashed(withdrawalId, totalSlashed);
    }

    // --- View Helpers ---

    /// @notice Returns the number of registered operators (including inactive).
    function operatorCount() external view returns (uint256) {
        return operatorList.length;
    }

    /// @notice Returns the number of currently active operators.
    function activeOperatorCount() external view returns (uint256) {
        uint256 count = 0;
        for (uint256 i = 0; i < operatorList.length; i++) {
            if (operators[operatorList[i]].active) count++;
        }
        return count;
    }
}
