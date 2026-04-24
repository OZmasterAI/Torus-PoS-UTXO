// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Test.sol";
import "../src/BridgeController.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20.sol";

// --- Mock Contracts ---

/// @dev Mock ERC20 token with public mint for testing
contract MockWTRS is ERC20 {
    constructor() ERC20("Mock wTRS", "mwTRS") {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function decimals() public pure override returns (uint8) {
        return 8;
    }
}

/// @dev Mock verifier that always returns true
contract MockVerifierPass is IVerifier {
    function verifyDeposit(
        bytes calldata,
        bytes32,
        bytes32,
        uint256,
        address
    ) external pure override returns (bool) {
        return true;
    }
}

/// @dev Mock verifier that always returns false
contract MockVerifierFail is IVerifier {
    function verifyDeposit(
        bytes calldata,
        bytes32,
        bytes32,
        uint256,
        address
    ) external pure override returns (bool) {
        return false;
    }
}

// --- Test Contract ---

contract BridgeControllerTest is Test {
    BridgeController public controller;
    MockWTRS public wTRS;
    MockVerifierPass public verifierPass;
    MockVerifierFail public verifierFail;

    address public operator1 = address(0x1001);
    address public operator2 = address(0x1002);
    address public user = address(0x2001);

    uint256 constant MIN_STAKE = 1 ether;
    uint256 constant WITHDRAWAL_TIMEOUT = 1 days;
    uint256 constant UNSTAKE_COOLDOWN = 7 days;

    function setUp() public {
        wTRS = new MockWTRS();
        verifierPass = new MockVerifierPass();
        verifierFail = new MockVerifierFail();

        controller = new BridgeController(
            IVerifier(address(verifierPass)),
            IERC20(address(wTRS)),
            MIN_STAKE,
            WITHDRAWAL_TIMEOUT,
            UNSTAKE_COOLDOWN
        );

        // Fund operators
        vm.deal(operator1, 10 ether);
        vm.deal(operator2, 10 ether);

        // Give user some wTRS and fund for gas
        wTRS.mint(user, 100e8);
        vm.deal(user, 1 ether);
    }

    // ========================================
    // Operator Registration
    // ========================================

    function test_registerOperator() public {
        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();

        (uint256 stake, uint256 unstakeInitiated, bool active) = controller.operators(operator1);
        assertEq(stake, 2 ether, "stake recorded");
        assertEq(unstakeInitiated, 0, "no unstake");
        assertTrue(active, "operator active");
        assertEq(controller.operatorCount(), 1, "one operator");
    }

    function test_registerOperator_emitsEvent() public {
        vm.expectEmit(true, false, false, true);
        emit BridgeController.OperatorRegistered(operator1, 2 ether);

        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();
    }

    function test_registerOperator_insufficientStake() public {
        vm.prank(operator1);
        vm.expectRevert(BridgeController.InsufficientStake.selector);
        controller.registerOperator{value: 0.5 ether}();
    }

    function test_registerOperator_alreadyRegistered() public {
        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();

        vm.prank(operator1);
        vm.expectRevert(BridgeController.AlreadyRegistered.selector);
        controller.registerOperator{value: 1 ether}();
    }

    function test_registerMultipleOperators() public {
        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();

        vm.prank(operator2);
        controller.registerOperator{value: 2 ether}();

        assertEq(controller.operatorCount(), 2, "two operators");
        assertEq(controller.activeOperatorCount(), 2, "two active");
    }

    // ========================================
    // Unstake Flow
    // ========================================

    function test_initiateUnstake() public {
        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();

        vm.prank(operator1);
        controller.initiateUnstake();

        (, uint256 unstakeInitiated, bool active) = controller.operators(operator1);
        assertEq(unstakeInitiated, block.timestamp, "unstake timestamp set");
        assertTrue(active, "still active during cooldown");
    }

    function test_initiateUnstake_notOperator() public {
        vm.prank(operator1);
        vm.expectRevert(BridgeController.NotOperator.selector);
        controller.initiateUnstake();
    }

    function test_completeUnstake() public {
        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();

        vm.prank(operator1);
        controller.initiateUnstake();

        // Warp past cooldown
        vm.warp(block.timestamp + UNSTAKE_COOLDOWN + 1);

        uint256 balBefore = operator1.balance;
        vm.prank(operator1);
        controller.completeUnstake();

        (uint256 stake, , bool active) = controller.operators(operator1);
        assertEq(stake, 0, "stake returned");
        assertFalse(active, "operator deactivated");
        assertEq(operator1.balance, balBefore + 2 ether, "ETH returned");
        assertEq(controller.activeOperatorCount(), 0, "no active operators");
    }

    function test_completeUnstake_cooldownNotElapsed() public {
        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();

        vm.prank(operator1);
        controller.initiateUnstake();

        // Try to complete before cooldown
        vm.warp(block.timestamp + UNSTAKE_COOLDOWN - 1);

        vm.prank(operator1);
        vm.expectRevert(BridgeController.CooldownNotElapsed.selector);
        controller.completeUnstake();
    }

    function test_completeUnstake_notInitiated() public {
        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();

        vm.prank(operator1);
        vm.expectRevert(BridgeController.UnstakeNotInitiated.selector);
        controller.completeUnstake();
    }

    // ========================================
    // Withdrawal Flow
    // ========================================

    function _requestWithdrawal(uint256 amount) internal returns (bytes32) {
        bytes20 torusAddr = bytes20(hex"89abcdefabbaabbaabbaabbaabbaabbaabbaabba");

        vm.prank(user);
        wTRS.approve(address(controller), amount);

        vm.prank(user);
        vm.recordLogs();
        controller.requestWithdrawal(amount, torusAddr);

        Vm.Log[] memory logs = vm.getRecordedLogs();
        // WithdrawalRequested event is the last log entry from controller
        bytes32 withdrawalId;
        for (uint256 i = logs.length; i > 0; i--) {
            if (logs[i - 1].emitter == address(controller)) {
                withdrawalId = logs[i - 1].topics[1]; // indexed id
                break;
            }
        }
        return withdrawalId;
    }

    function test_requestWithdrawal() public {
        uint256 amount = 10e8;
        uint256 balBefore = wTRS.balanceOf(user);

        bytes32 id = _requestWithdrawal(amount);
        assertTrue(id != bytes32(0), "withdrawal id generated");

        // wTRS transferred from user to controller
        assertEq(wTRS.balanceOf(user), balBefore - amount, "user balance decreased");
        assertEq(wTRS.balanceOf(address(controller)), amount, "controller holds wTRS");

        // Check stored request
        (
            address requester,
            uint256 wAmount,
            bytes20 torusAddress,
            uint256 requestedAt,
            uint256 deadline,
            bool completed
        ) = controller.withdrawals(id);

        assertEq(requester, user, "requester stored");
        assertEq(wAmount, amount, "amount stored");
        assertTrue(torusAddress != bytes20(0), "torus address stored");
        assertEq(requestedAt, block.timestamp, "requestedAt set");
        assertEq(deadline, block.timestamp + WITHDRAWAL_TIMEOUT, "deadline set");
        assertFalse(completed, "not yet completed");
    }

    function test_requestWithdrawal_zeroAmount() public {
        vm.prank(user);
        vm.expectRevert(BridgeController.ZeroAmount.selector);
        controller.requestWithdrawal(0, bytes20(hex"1234567890123456789012345678901234567890"));
    }

    function test_requestWithdrawal_noApproval() public {
        // No approval given -- should revert on transferFrom
        vm.prank(user);
        vm.expectRevert();
        controller.requestWithdrawal(10e8, bytes20(hex"1234567890123456789012345678901234567890"));
    }

    // ========================================
    // Confirm Withdrawal
    // ========================================

    function test_confirmWithdrawal() public {
        uint256 amount = 10e8;
        bytes32 id = _requestWithdrawal(amount);

        // Confirm with mock proof (verifier always passes)
        bytes memory proof = hex"deadbeef";
        bytes32 blockHash = keccak256("block1");
        bytes32 txHash = keccak256("tx1");

        controller.confirmWithdrawal(id, proof, blockHash, txHash);

        (, , , , , bool completed) = controller.withdrawals(id);
        assertTrue(completed, "withdrawal completed");
    }

    function test_confirmWithdrawal_emitsEvent() public {
        uint256 amount = 10e8;
        bytes32 id = _requestWithdrawal(amount);

        bytes memory proof = hex"deadbeef";
        bytes32 blockHash = keccak256("block1");
        bytes32 txHash = keccak256("tx1");

        vm.expectEmit(true, false, false, true);
        emit BridgeController.WithdrawalCompleted(id, txHash);

        controller.confirmWithdrawal(id, proof, blockHash, txHash);
    }

    function test_confirmWithdrawal_doubleComplete() public {
        uint256 amount = 10e8;
        bytes32 id = _requestWithdrawal(amount);

        bytes memory proof = hex"deadbeef";
        bytes32 blockHash = keccak256("block1");
        bytes32 txHash = keccak256("tx1");

        controller.confirmWithdrawal(id, proof, blockHash, txHash);

        // Attempt to complete again
        vm.expectRevert(BridgeController.WithdrawalAlreadyCompleted.selector);
        controller.confirmWithdrawal(id, proof, blockHash, txHash);
    }

    function test_confirmWithdrawal_notFound() public {
        bytes32 fakeId = keccak256("nonexistent");

        vm.expectRevert(BridgeController.WithdrawalNotFound.selector);
        controller.confirmWithdrawal(fakeId, hex"", bytes32(0), bytes32(0));
    }

    function test_confirmWithdrawal_invalidProof() public {
        // Deploy controller with failing verifier
        BridgeController failController = new BridgeController(
            IVerifier(address(verifierFail)),
            IERC20(address(wTRS)),
            MIN_STAKE,
            WITHDRAWAL_TIMEOUT,
            UNSTAKE_COOLDOWN
        );

        // Setup: give user tokens and approve
        uint256 amount = 5e8;
        bytes20 torusAddr = bytes20(hex"89abcdefabbaabbaabbaabbaabbaabbaabbaabba");

        vm.prank(user);
        wTRS.approve(address(failController), amount);

        vm.prank(user);
        vm.recordLogs();
        failController.requestWithdrawal(amount, torusAddr);

        Vm.Log[] memory logs = vm.getRecordedLogs();
        bytes32 id;
        for (uint256 i = logs.length; i > 0; i--) {
            if (logs[i - 1].emitter == address(failController)) {
                id = logs[i - 1].topics[1];
                break;
            }
        }

        vm.expectRevert(BridgeController.InvalidProof.selector);
        failController.confirmWithdrawal(id, hex"badd", keccak256("b"), keccak256("t"));
    }

    // ========================================
    // Slashing
    // ========================================

    function test_slashForTimeout() public {
        // Register two operators
        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();

        vm.prank(operator2);
        controller.registerOperator{value: 4 ether}();

        // Request withdrawal
        uint256 amount = 10e8;
        bytes32 id = _requestWithdrawal(amount);

        // Warp past deadline
        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        uint256 userBalBefore = user.balance;

        controller.slashForTimeout(id);

        // Each operator loses 50%: op1 loses 1 ETH, op2 loses 2 ETH = 3 ETH total
        (uint256 stake1, , ) = controller.operators(operator1);
        (uint256 stake2, , ) = controller.operators(operator2);
        assertEq(stake1, 1 ether, "op1 stake halved");
        assertEq(stake2, 2 ether, "op2 stake halved");

        // User receives slashed ETH
        assertEq(user.balance, userBalBefore + 3 ether, "user received slashed ETH");

        // Withdrawal marked completed (prevents re-slash)
        (, , , , , bool completed) = controller.withdrawals(id);
        assertTrue(completed, "marked complete after slash");
    }

    function test_slashForTimeout_emitsEvent() public {
        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();

        uint256 amount = 5e8;
        bytes32 id = _requestWithdrawal(amount);

        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        vm.expectEmit(true, false, false, true);
        emit BridgeController.OperatorSlashed(id, 1 ether); // 50% of 2 ETH

        controller.slashForTimeout(id);
    }

    function test_slashForTimeout_deadlineNotPassed() public {
        uint256 amount = 5e8;
        bytes32 id = _requestWithdrawal(amount);

        vm.expectRevert(BridgeController.DeadlineNotPassed.selector);
        controller.slashForTimeout(id);
    }

    function test_slashForTimeout_alreadyCompleted() public {
        uint256 amount = 5e8;
        bytes32 id = _requestWithdrawal(amount);

        // Confirm the withdrawal first
        controller.confirmWithdrawal(id, hex"ab", keccak256("b"), keccak256("t"));

        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        vm.expectRevert(BridgeController.WithdrawalAlreadyCompleted.selector);
        controller.slashForTimeout(id);
    }

    function test_slashForTimeout_noActiveOperators() public {
        uint256 amount = 5e8;
        bytes32 id = _requestWithdrawal(amount);

        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        uint256 userBalBefore = user.balance;

        // No operators registered, so nothing to slash
        controller.slashForTimeout(id);

        // No ETH sent (nothing slashed)
        assertEq(user.balance, userBalBefore, "no ETH transferred");
    }

    function test_slashForTimeout_doesNotSlashInactiveOperator() public {
        // Register and then deactivate operator1
        vm.prank(operator1);
        controller.registerOperator{value: 2 ether}();

        vm.prank(operator1);
        controller.initiateUnstake();
        vm.warp(block.timestamp + UNSTAKE_COOLDOWN + 1);
        vm.prank(operator1);
        controller.completeUnstake();

        // Register operator2 (stays active)
        vm.prank(operator2);
        controller.registerOperator{value: 4 ether}();

        // Request withdrawal after operator2 is registered
        uint256 amount = 5e8;
        bytes32 id = _requestWithdrawal(amount);

        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        uint256 userBalBefore = user.balance;
        controller.slashForTimeout(id);

        // Only operator2 should be slashed (50% of 4 ETH = 2 ETH)
        (uint256 stake2, , ) = controller.operators(operator2);
        assertEq(stake2, 2 ether, "op2 halved");
        assertEq(user.balance, userBalBefore + 2 ether, "only active op slashed");
    }

    function test_slashDoesNotPreventDoubleSlashAcrossWithdrawals() public {
        // Each withdrawal can slash independently
        vm.prank(operator1);
        controller.registerOperator{value: 4 ether}();

        bytes32 id1 = _requestWithdrawal(5e8);
        bytes32 id2 = _requestWithdrawal(3e8);

        vm.warp(block.timestamp + WITHDRAWAL_TIMEOUT + 1);

        controller.slashForTimeout(id1);
        (uint256 stakeAfter1, , ) = controller.operators(operator1);
        assertEq(stakeAfter1, 2 ether, "first slash: 4 -> 2");

        controller.slashForTimeout(id2);
        (uint256 stakeAfter2, , ) = controller.operators(operator1);
        assertEq(stakeAfter2, 1 ether, "second slash: 2 -> 1");
    }

    // ========================================
    // View Helpers
    // ========================================

    function test_activeOperatorCount() public {
        assertEq(controller.activeOperatorCount(), 0, "none initially");

        vm.prank(operator1);
        controller.registerOperator{value: 1 ether}();
        assertEq(controller.activeOperatorCount(), 1, "one active");

        vm.prank(operator2);
        controller.registerOperator{value: 1 ether}();
        assertEq(controller.activeOperatorCount(), 2, "two active");

        // Deactivate operator1
        vm.prank(operator1);
        controller.initiateUnstake();
        vm.warp(block.timestamp + UNSTAKE_COOLDOWN + 1);
        vm.prank(operator1);
        controller.completeUnstake();

        assertEq(controller.activeOperatorCount(), 1, "back to one");
    }
}
