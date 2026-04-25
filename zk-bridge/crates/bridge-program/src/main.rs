// SP1 guest program for proving torus-core PoS block validity.
//
// This program runs inside the SP1 RISC-V zkVM. It reads block data as private
// inputs, verifies the PoS kernel hash and header chain, then commits the
// proven block hash and deposit info as public outputs.
//
// To build: `cargo prove build` (requires SP1 toolchain)

#![no_main]
sp1_zkvm::entrypoint!(main);

use torus_kernel::{
    BlockHeader, StakeKernelInput,
    check_stake_kernel_hash, hash_block_header, verify_header_chain,
};

/// Public outputs committed to the proof.
#[derive(Clone, Debug)]
pub struct BridgeProofOutput {
    pub block_hash: [u8; 32],
    pub block_height: u64,
    pub deposit_tx_hash: [u8; 32],
    pub deposit_amount: u64,
    pub recipient: [u8; 20],
}

fn main() {
    let headers: Vec<BlockHeader> = sp1_zkvm::io::read();
    let kernel_input: StakeKernelInput = sp1_zkvm::io::read();
    let deposit_tx_hash: [u8; 32] = sp1_zkvm::io::read();
    let merkle_proof: Vec<([u8; 32], bool)> = sp1_zkvm::io::read();
    let deposit_amount: u64 = sp1_zkvm::io::read();
    let recipient: [u8; 20] = sp1_zkvm::io::read();

    // Step 1: Verify header chain continuity
    assert!(verify_header_chain(&headers), "invalid header chain");

    // Step 2: Verify the PoS kernel hash for the deposit block
    let result = check_stake_kernel_hash(&kernel_input);
    assert!(result.meets_target, "stake kernel hash does not meet target");

    // Step 3: Verify deposit tx is in the block's merkle tree
    let deposit_block = headers.last().unwrap();
    assert!(
        torus_kernel::verify_merkle_proof(
            &deposit_tx_hash,
            &merkle_proof,
            &deposit_block.hash_merkle_root,
        ),
        "deposit tx not in merkle tree"
    );

    // Step 4: Commit public outputs
    let block_hash = hash_block_header(deposit_block);
    let kernel_hash = result.kernel_hash;
    sp1_zkvm::io::commit(&block_hash);
    sp1_zkvm::io::commit(&kernel_hash);
    sp1_zkvm::io::commit(&deposit_tx_hash);
    sp1_zkvm::io::commit(&deposit_amount);
    sp1_zkvm::io::commit(&recipient);
}
