#![no_main]
sp1_zkvm::entrypoint!(main);

use torus_kernel::{
    BlockHeader, StakeKernelInput,
    check_stake_kernel_hash, hash_block_header, verify_merkle_proof,
};

pub fn main() {
    let mode: u8 = sp1_zkvm::io::read();
    assert!(mode <= 1, "invalid mode: must be 0 (deposit) or 1 (withdrawal)");

    // --- Private inputs (same for both modes) ---
    let header: BlockHeader = sp1_zkvm::io::read();
    let kernel_input: StakeKernelInput = sp1_zkvm::io::read();
    let deposit_tx_hash: [u8; 32] = sp1_zkvm::io::read();
    let merkle_proof: Vec<([u8; 32], bool)> = sp1_zkvm::io::read();
    let amount: u64 = sp1_zkvm::io::read();
    let recipient: [u8; 20] = sp1_zkvm::io::read();

    // 1. Block header hash
    let block_hash = hash_block_header(&header);

    // 2. PoS kernel hash must meet difficulty target
    let kernel_output = check_stake_kernel_hash(&kernel_input);
    assert!(kernel_output.meets_target, "PoS kernel hash does not meet target");

    // 3. Deposit tx must be in the block's merkle tree
    assert!(
        verify_merkle_proof(&deposit_tx_hash, &merkle_proof, &header.hash_merkle_root),
        "merkle inclusion proof invalid",
    );

    // --- Public outputs (160 bytes, big-endian for Solidity) ---
    let mut block_hash_be = block_hash;
    block_hash_be.reverse();
    sp1_zkvm::io::commit_slice(&block_hash_be);

    let mut kernel_hash_be = kernel_output.hash_proof_of_stake;
    kernel_hash_be.reverse();
    sp1_zkvm::io::commit_slice(&kernel_hash_be);

    let mut tx_hash_be = deposit_tx_hash;
    tx_hash_be.reverse();
    sp1_zkvm::io::commit_slice(&tx_hash_be);

    let mut amount_bytes = [0u8; 32];
    amount_bytes[24..32].copy_from_slice(&amount.to_be_bytes());
    sp1_zkvm::io::commit_slice(&amount_bytes);

    let mut recipient_bytes = [0u8; 32];
    recipient_bytes[12..32].copy_from_slice(&recipient);
    sp1_zkvm::io::commit_slice(&recipient_bytes);
}
