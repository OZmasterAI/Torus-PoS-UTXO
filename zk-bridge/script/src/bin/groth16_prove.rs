use sp1_sdk::blocking::{Elf, ProveRequest, Prover, ProverClient, SP1Stdin};
use sp1_sdk::ProvingKey;
use std::time::Instant;
use torus_kernel::{
    BlockHeader, StakeKernelInput, check_stake_kernel_hash, double_sha256, hash_block_header,
    COIN, STAKE_MAX_AGE, STAKE_MIN_AGE,
};

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
    let deposit_tx_hash = double_sha256(b"torus_deposit:1000TRS:bridge:TBridgeAddr1234");
    let coinbase_tx_hash = double_sha256(b"coinbase_tx_reward");

    let mut combined = Vec::with_capacity(64);
    combined.extend_from_slice(&deposit_tx_hash);
    combined.extend_from_slice(&coinbase_tx_hash);
    let merkle_root = double_sha256(&combined);

    let merkle_proof: Vec<([u8; 32], bool)> = vec![(coinbase_tx_hash, true)];

    let header = BlockHeader {
        version: 1,
        hash_prev_block: hex_to_internal_bytes(
            "000005a39de532e9f2546ad8c954a21f01e0064f3edc9fea108f39e0499a011d",
        ),
        hash_merkle_root: merkle_root,
        time: 1700100000,
        bits: 0x207fffff,
        nonce: 42,
    };
    let block_hash = hash_block_header(&header);

    let kernel_input = StakeKernelInput {
        n_bits: 0x207fffff,
        stake_modifier: 0x1234,
        block_time_from: 1700000000,
        tx_prev_offset: 200,
        tx_prev_time: 1700000000,
        prevout_n: 0,
        time_tx: 1700000000 + STAKE_MIN_AGE + STAKE_MAX_AGE as u32,
        value_in: 50_000_000 * COIN,
        is_permanent_stake: false,
    };

    let kernel_output = check_stake_kernel_hash(&kernel_input);
    assert!(kernel_output.meets_target, "kernel must meet target");

    let amount: u64 = 1000 * COIN;
    let recipient: [u8; 20] = [0x42; 20];

    println!("=== Groth16 Proof Generation ===\n");
    println!("Block hash:  {}", hex::encode(&block_hash));
    println!("Amount:      {} TRS", amount as f64 / COIN as f64);

    let elf_bytes = include_bytes!("../../../program/elf/riscv32im-succinct-zkvm-elf");
    let elf: Elf = elf_bytes.as_slice().into();

    let mut stdin = SP1Stdin::new();
    stdin.write(&0u8);
    stdin.write(&header);
    stdin.write(&kernel_input);
    stdin.write(&deposit_tx_hash);
    stdin.write(&merkle_proof);
    stdin.write(&amount);
    stdin.write(&recipient);

    #[cfg(feature = "network")]
    let client = {
        println!("Using SP1 Prover Network");
        ProverClient::builder().network().build()
    };
    #[cfg(all(feature = "cuda", not(feature = "network")))]
    let client = {
        println!("Using CUDA prover");
        ProverClient::builder().cuda().build()
    };
    #[cfg(all(not(feature = "cuda"), not(feature = "network")))]
    let client = {
        println!("Using CPU prover");
        ProverClient::builder().cpu().build()
    };

    println!("\nSetting up proving key...");
    let t0 = Instant::now();
    let pk = client.setup(elf.clone()).expect("setup failed");
    let vk = pk.verifying_key().clone();
    println!("  Setup: {:.2?}", t0.elapsed());

    println!("\nGenerating Groth16 proof (this takes a while)...");
    let t0 = Instant::now();
    let proof = client
        .prove(&pk, stdin)
        .groth16()
        .run()
        .expect("groth16 proving failed");
    let prove_time = t0.elapsed();
    println!("  Proving: {:.2?}", prove_time);

    println!("\nVerifying...");
    let t0 = Instant::now();
    client.verify(&proof, &vk, None).expect("verification failed");
    println!("  Verify: {:.2?}", t0.elapsed());

    let public_values = proof.public_values.as_slice();
    let proof_bytes = proof.bytes();

    println!("\n=== Output Files ===");

    std::fs::write("groth16_public_values.bin", public_values).expect("write public values");
    std::fs::write("groth16_proof.bin", &proof_bytes).expect("write proof");

    let full_proof_json = serde_json::to_string_pretty(&proof).expect("serialize proof");
    std::fs::write("groth16_proof.json", &full_proof_json).expect("write json");

    // Concatenated format for on-chain submission: [publicValues | proofBytes]
    let mut onchain_calldata = Vec::with_capacity(public_values.len() + proof_bytes.len());
    onchain_calldata.extend_from_slice(public_values);
    onchain_calldata.extend_from_slice(&proof_bytes);
    let onchain_hex = format!("0x{}", hex::encode(&onchain_calldata));
    std::fs::write("groth16_onchain_proof.hex", &onchain_hex).expect("write onchain hex");

    println!("  groth16_public_values.bin  ({} bytes)", public_values.len());
    println!("  groth16_proof.bin          ({} bytes)", proof_bytes.len());
    println!("  groth16_proof.json         (full serialized proof)");
    println!("  groth16_onchain_proof.hex  (concatenated for Solidity)");

    println!("\n=== Summary ===");
    println!("  Groth16 prove time: {:.2?}", prove_time);
    println!("  Public values:      {} bytes", public_values.len());
    println!("  Proof bytes:        {} bytes", proof_bytes.len());
    println!("  On-chain calldata:  {} bytes", onchain_calldata.len());
    println!("\n  Copy groth16_onchain_proof.hex back to your machine.");
    println!("  This is the `proof` parameter for WrappedTRS.mint()");
    println!("\n=== Done ===");
}
