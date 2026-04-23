use sp1_sdk::blocking::{Elf, ProveRequest, Prover, ProverClient, SP1Stdin};
use sp1_sdk::ProvingKey;
use torus_kernel::BlockHeader;

fn genesis_header() -> BlockHeader {
    BlockHeader {
        version: 1,
        hash_prev_block: [0u8; 32],
        hash_merkle_root: hex_to_internal_bytes(
            "cdc376c01136ce03cbdf5c6faa1eeaaddaff9d2e40a4fdb2825b1cff8e123de6",
        ),
        time: 1638617750,
        bits: 0x1e0fffff,
        nonce: 627293,
    }
}

fn genesis_hash() -> [u8; 32] {
    hex_to_internal_bytes(
        "000005a39de532e9f2546ad8c954a21f01e0064f3edc9fea108f39e0499a011d",
    )
}

fn hex_to_internal_bytes(hex_str: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_str).expect("invalid hex");
    assert_eq!(bytes.len(), 32);
    let mut result = [0u8; 32];
    for i in 0..32 {
        result[i] = bytes[31 - i];
    }
    result
}

fn main() {
    let header = genesis_header();
    let expected_hash = genesis_hash();

    println!("=== Torus-Core ZK Bridge PoC ===");
    println!("Proving genesis block header hash...");
    println!("  nTime:  {}", header.time);
    println!("  nBits:  0x{:08x}", header.bits);
    println!("  nNonce: {}", header.nonce);

    let elf_bytes = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
    let elf: Elf = elf_bytes.as_slice().into();

    let mut stdin = SP1Stdin::new();
    stdin.write(&header);
    stdin.write(&expected_hash);

    let client = ProverClient::builder().cpu().build();

    println!("\nExecuting program (no proof)...");
    let (public_values, report) = client
        .execute(elf.clone(), stdin.clone())
        .run()
        .expect("execution failed");
    println!(
        "  Execution OK — {} cycles",
        report.total_instruction_count()
    );
    println!(
        "  Committed block hash: {}",
        internal_bytes_to_hex(public_values.as_slice())
    );

    println!("\nSetting up proving key...");
    let pk = client.setup(elf).expect("setup failed");
    let vk = pk.verifying_key().clone();

    println!("Generating proof...");
    let proof = client
        .prove(&pk, stdin)
        .run()
        .expect("proof generation failed");

    println!("  Proof generated!");
    println!(
        "  Committed block hash: {}",
        internal_bytes_to_hex(proof.public_values.as_slice())
    );

    println!("\nVerifying proof...");
    client
        .verify(&proof, &vk, None)
        .expect("verification failed");
    println!("  Proof verified!");

    println!("\n=== PoC Complete ===");
    println!("Successfully proved that a torus-core block header hashes to the");
    println!("expected genesis hash using a ZK proof (SP1 STARK).");
}

fn internal_bytes_to_hex(bytes: &[u8]) -> String {
    let mut reversed: Vec<u8> = bytes.to_vec();
    reversed.reverse();
    hex::encode(reversed)
}
