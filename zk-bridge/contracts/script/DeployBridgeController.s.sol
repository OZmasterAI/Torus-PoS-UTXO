// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/BridgeController.sol";

contract DeployBridgeController is Script {
    address constant SP1_VERIFIER = 0xcc3D3315dD66B38Ca87FD31822d1B6706dfFadEF;
    address constant WTRS = 0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a;

    uint256 constant MIN_STAKE = 0.01 ether;
    uint256 constant MIN_WITHDRAWAL = 1e6; // 0.01 TRS (8 decimals)
    uint256 constant WITHDRAWAL_TIMEOUT = 1 hours;
    uint256 constant UNSTAKE_COOLDOWN = 1 hours;

    function run() external {
        uint256 deployerKey = vm.envUint("BSC_TEST_KEY");
        address deployer = vm.addr(deployerKey);

        console.log("Deployer:", deployer);
        console.log("Balance:", deployer.balance);

        vm.startBroadcast(deployerKey);

        BridgeController controller = new BridgeController(
            IVerifier(SP1_VERIFIER),
            IERC20Burnable(WTRS),
            MIN_STAKE,
            MIN_WITHDRAWAL,
            WITHDRAWAL_TIMEOUT,
            UNSTAKE_COOLDOWN
        );
        console.log("BridgeController:", address(controller));

        controller.registerOperator{value: MIN_STAKE}();
        console.log("Deployer registered as operator with", MIN_STAKE, "wei stake");

        vm.stopBroadcast();

        console.log("\n=== BridgeController Deployment Complete ===");
        console.log("Network: Sepolia (chainId 11155111)");
        console.log("SP1Verifier (withdrawalVerifier):", SP1_VERIFIER);
        console.log("WrappedTRS (token):", WTRS);
        console.log("Min stake:", MIN_STAKE);
        console.log("Withdrawal timeout: 1 hour");
        console.log("Unstake cooldown: 1 hour");
        console.log("\nSet BRIDGE_CONTROLLER_ADDRESS=<address> in relayer .env");
    }
}
