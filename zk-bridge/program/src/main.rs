#![no_main]
sp1_zkvm::entrypoint!(main);

use torus_kernel::{BlockHeader, double_sha256, hash_block_header, verify_header_chain};

pub fn main() {
    let header: BlockHeader = sp1_zkvm::io::read();

    let computed_hash = hash_block_header(&header);

    let expected_hash: [u8; 32] = sp1_zkvm::io::read();
    assert_eq!(computed_hash, expected_hash, "block hash mismatch");

    sp1_zkvm::io::commit_slice(&computed_hash);
}
