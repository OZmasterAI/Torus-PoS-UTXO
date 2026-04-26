// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/WrappedTRS.sol";

contract ZKMint is Script {
    address constant WTRS = 0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a;
    address constant SP1_VERIFIER = 0xcc3D3315dD66B38Ca87FD31822d1B6706dfFadEF;

    function run() external {
        uint256 deployerKey = vm.envUint("BSC_TEST_KEY");
        address deployer = vm.addr(deployerKey);

        WrappedTRS token = WrappedTRS(WTRS);

        console.log("=== ZK Mint on Sepolia ===");
        console.log("Deployer:", deployer);
        console.log("Current verifier:", address(token.verifier()));

        // Step 1: Swap to SP1Verifier
        console.log("\n[1] Setting SP1Verifier...");
        vm.startBroadcast(deployerKey);
        token.setVerifier(SP1_VERIFIER);
        vm.stopBroadcast();
        console.log("  Verifier set to:", address(token.verifier()));

        // Step 2: Read the Groth16 proof from file
        bytes memory proof = vm.readFileBinary("groth16_onchain_proof.bin");
        console.log("\n[2] Proof loaded:", proof.length, "bytes");

        // Public values layout (first 192 bytes of proof):
        //   [0:32]   mode (0 = deposit, 1 = withdrawal)
        //   [32:64]  blockHash
        //   [64:96]  kernelHash
        //   [96:128] txHash
        //   [128:160] amount
        //   [160:192] recipient (left-padded address)
        uint256 mode;
        bytes32 blockHash;
        bytes32 txHash;
        uint256 amount;
        uint256 recipientWord;
        assembly {
            mode := mload(add(proof, 32))
            blockHash := mload(add(proof, 64))
            txHash := mload(add(proof, 128))
            amount := mload(add(proof, 160))
            recipientWord := mload(add(proof, 192))
        }
        require(mode == 0, "proof is not a deposit proof");
        address recipient = address(uint160(recipientWord));

        console.log("  Block hash:", vm.toString(blockHash));
        console.log("  TX hash:", vm.toString(txHash));
        console.log("  Amount:", amount);
        console.log("  Recipient:", recipient);

        // Step 3: Mint with ZK proof
        console.log("\n[3] Minting wTRS with Groth16 proof...");
        vm.startBroadcast(deployerKey);
        token.mint(proof, blockHash, txHash, amount, recipient);
        vm.stopBroadcast();

        console.log("\n=== ZK Mint Successful! ===");
        console.log("  wTRS balance:", token.balanceOf(recipient));
        console.log("  Deposit processed:", token.processedDeposits(txHash) ? "true" : "false");
    }
}
