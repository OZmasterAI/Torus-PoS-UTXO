// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/WrappedTRS.sol";

contract DeploySepolia is Script {
    // SP1 Groth16 VerifierGateway on Sepolia (deployed by Succinct)
    address constant SP1_GATEWAY = 0x397A5f7f3dBd538f23DE225B51f532c34448dA9B;

    // vKey for our torus-bridge SP1 program (from `cargo prove vkey`)
    bytes32 constant PROGRAM_VKEY = 0x0083951d6e5d5cebf863e820c2317a8648861df8dc3f373a926a9e52df274dda;

    function run() external {
        uint256 deployerKey = vm.envUint("BSC_TEST_KEY");
        address deployer = vm.addr(deployerKey);

        console.log("Deployer:", deployer);
        console.log("Balance:", deployer.balance);

        vm.startBroadcast(deployerKey);

        // 1. Deploy ThresholdVerifier (1-of-1 for testing)
        address[] memory signers = new address[](1);
        signers[0] = deployer;
        ThresholdVerifier thresholdVerifier = new ThresholdVerifier(signers, 1);
        console.log("ThresholdVerifier:", address(thresholdVerifier));

        // 2. Deploy WrappedTRS with ThresholdVerifier as initial verifier
        WrappedTRS token = new WrappedTRS(
            address(thresholdVerifier),
            1e10,   // minDeposit: 100 TRS (8 decimals)
            1e14    // maxDeposit: 1,000,000 TRS
        );
        console.log("WrappedTRS (wTRS):", address(token));

        // 3. Deploy SP1Verifier pointing to Succinct's gateway
        SP1Verifier sp1Verifier = new SP1Verifier(SP1_GATEWAY, PROGRAM_VKEY);
        console.log("SP1Verifier:", address(sp1Verifier));

        vm.stopBroadcast();

        console.log("\n=== Sepolia Deployment Complete ===");
        console.log("Network: Sepolia (chainId 11155111)");
        console.log("SP1 Gateway:", SP1_GATEWAY);
        console.log("Program vKey:", vm.toString(PROGRAM_VKEY));
        console.log("\nTo upgrade to ZK verification:");
        console.log("  token.setVerifier(sp1VerifierAddress)");
    }
}
