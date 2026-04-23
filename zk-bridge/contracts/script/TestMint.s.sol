// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "forge-std/Script.sol";
import "../src/WrappedTRS.sol";

contract TestMint is Script {
    function run() external {
        uint256 deployerKey = vm.envUint("BSC_TEST_KEY");
        address deployer = vm.addr(deployerKey);

        address tokenAddr = 0x6e62cBEdBcaA8057DDBaB36CC83Ea8Fde8dB9581;
        WrappedTRS token = WrappedTRS(tokenAddr);

        // Simulated torus-core deposit details
        bytes32 blockHash = keccak256("torus-block-12345");
        bytes32 txHash = keccak256("torus-deposit-tx-1");
        uint256 amount = 1000 * 1e8; // 1000 TRS (8 decimals)
        address recipient = deployer;

        // Create threshold signature (1-of-1: deployer signs the deposit attestation)
        bytes32 message = keccak256(abi.encodePacked(blockHash, txHash, amount, recipient));
        bytes32 ethMessage = keccak256(abi.encodePacked("\x19Ethereum Signed Message:\n32", message));
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(deployerKey, ethMessage);
        bytes memory proof = abi.encodePacked(r, s, v);

        console.log("=== Test Mint on BSC Testnet ===");
        console.log("Recipient:", recipient);
        console.log("Amount: 1000 TRS (100000000000 base units)");
        console.log("Block hash:", vm.toString(blockHash));
        console.log("TX hash:", vm.toString(txHash));

        uint256 gasBefore = gasleft();

        vm.startBroadcast(deployerKey);
        token.mint(proof, blockHash, txHash, amount, recipient);
        vm.stopBroadcast();

        console.log("\n=== Mint Successful ===");
        console.log("wTRS balance:", token.balanceOf(recipient));
        console.log("Deposit processed:", token.processedDeposits(txHash) ? "true" : "false");
    }
}
