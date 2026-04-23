// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/WrappedTRS.sol";

contract DeployBridge is Script {
    function run() external {
        uint256 deployerKey = vm.envUint("BSC_TEST_KEY");
        address deployer = vm.addr(deployerKey);

        console.log("Deployer:", deployer);
        console.log("Balance:", deployer.balance);

        vm.startBroadcast(deployerKey);

        // Deploy ThresholdVerifier with deployer as sole signer (1-of-1 for testing)
        address[] memory signers = new address[](1);
        signers[0] = deployer;
        ThresholdVerifier verifier = new ThresholdVerifier(signers, 1);
        console.log("ThresholdVerifier:", address(verifier));

        // Deploy WrappedTRS: min 100 TRS (1e10 base), max 1M TRS (1e14 base)
        WrappedTRS token = new WrappedTRS(
            address(verifier),
            1e10,   // minDeposit: 100 TRS
            1e14    // maxDeposit: 1,000,000 TRS
        );
        console.log("WrappedTRS (wTRS):", address(token));

        vm.stopBroadcast();

        console.log("\n=== Deployment Complete ===");
        console.log("Network: BSC Testnet (chainId 97)");
        console.log("Verifier: ThresholdVerifier (1-of-1, test mode)");
        console.log("Token: wTRS (8 decimals, matching torus-core)");
    }
}
