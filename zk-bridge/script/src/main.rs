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

fn bytes_to_display_hex(bytes: &[u8]) -> String {
    let mut reversed: Vec<u8> = bytes.to_vec();
    reversed.reverse();
    hex::encode(reversed)
}

fn main() {
    // === Build synthetic bridge deposit scenario ===

    let deposit_tx_hash = double_sha256(b"torus_deposit:1000TRS:bridge:TBridgeAddr1234");
    let coinbase_tx_hash = double_sha256(b"coinbase_tx_reward");

    // 2-leaf merkle tree: deposit_tx || coinbase_tx
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
    assert!(kernel_output.meets_target, "test kernel must meet easy target");

    let amount: u64 = 1000 * COIN;
    let recipient: [u8; 20] = [0x42; 20];

    println!("=== Torus-Core ZK Bridge — Full Circuit ===\n");
    println!("Block hash:    {}", bytes_to_display_hex(&block_hash));
    println!("Kernel hash:   {}", bytes_to_display_hex(&kernel_output.hash_proof_of_stake));
    println!("Deposit tx:    {}", bytes_to_display_hex(&deposit_tx_hash));
    println!("Amount:        {} TRS", amount as f64 / COIN as f64);
    println!("Recipient:     0x{}", hex::encode(recipient));
    println!("Merkle levels: {}", merkle_proof.len());

    // === Load SP1 program ===
    let elf_bytes = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
    let elf: Elf = elf_bytes.as_slice().into();

    let mut stdin = SP1Stdin::new();
    stdin.write(&0u8);
    stdin.write(&header);
    stdin.write(&kernel_input);
    stdin.write(&deposit_tx_hash);
    stdin.write(&merkle_proof);
    stdin.write(&amount);
    stdin.write(&recipient);

    let client = ProverClient::builder().cpu().build();

    // === Phase 1: Execute (no proof) — measure cycles ===
    println!("\n--- Phase 1: Execution (no proof) ---");
    let t0 = Instant::now();
    let (public_values, report) = client
        .execute(elf.clone(), stdin.clone())
        .run()
        .expect("execution failed");
    let exec_time = t0.elapsed();

    let cycles = report.total_instruction_count();
    println!("  Cycles:    {}", cycles);
    println!("  Time:      {:.2?}", exec_time);

    let pv = public_values.as_slice();
    println!("  Public outputs: {} bytes", pv.len());
    if pv.len() >= 192 {
        println!("    Mode:        {}", pv[31]);
        println!("    Block hash:  {}", hex::encode(&pv[32..64]));
        println!("    Kernel hash: {}", hex::encode(&pv[64..96]));
        println!("    TX hash:     {}", hex::encode(&pv[96..128]));
        let amount_word = &pv[128..160];
        let proven_amount = u64::from_be_bytes(amount_word[24..32].try_into().unwrap());
        println!("    Amount:      {} TRS", proven_amount as f64 / COIN as f64);
        println!("    Recipient:   0x{}", hex::encode(&pv[172..192]));
    }

    // === Phase 2: STARK proof generation ===
    println!("\n--- Phase 2: STARK proof ---");
    let t0 = Instant::now();
    let pk = client.setup(elf.clone()).expect("setup failed");
    let vk = pk.verifying_key().clone();
    let setup_time = t0.elapsed();
    println!("  Setup:     {:.2?}", setup_time);

    let t0 = Instant::now();
    let proof = client.prove(&pk, stdin.clone()).run().expect("proving failed");
    let prove_time = t0.elapsed();
    let proof_size = serde_json::to_vec(&proof).map(|v| v.len()).unwrap_or(0);
    println!("  Proving:   {:.2?}", prove_time);
    println!("  Proof size: {} bytes ({:.1} KB)", proof_size, proof_size as f64 / 1024.0);

    // === Phase 3: Verify ===
    println!("\n--- Phase 3: Verification ---");
    let t0 = Instant::now();
    client.verify(&proof, &vk, None).expect("verification failed");
    let verify_time = t0.elapsed();
    println!("  Verify:    {:.2?}", verify_time);

    // === Summary ===
    println!("\n=== Benchmark Summary ===");
    println!("Circuit: header hash + PoS kernel check + Merkle inclusion");
    println!("  Prev baseline (header only): 21,604 cycles");
    println!("  Full circuit cycles:         {}", cycles);
    println!(
        "  Overhead:                    {:.1}x",
        cycles as f64 / 21604.0
    );
    println!("  STARK proof time:            {:.2?}", prove_time);
    println!("  STARK proof size:            {:.1} KB", proof_size as f64 / 1024.0);
    println!("  Verification time:           {:.2?}", verify_time);

    println!("\n=== Groth16 On-Chain Estimates ===");
    println!("  Pipeline:       STARK → recursion → Groth16 (BN254)");
    println!("  Proof size:     ~260 bytes (a, b, c curve points)");
    println!("  Verify gas:     ~270k (ecPairing precompile)");
    println!("  + ERC-20 mint:  ~50k gas");
    println!("  Total per mint: ~320k gas");
    println!("  BSC testnet:    ~320k × 5 gwei = ~0.0016 tBNB");

    println!("\n  To generate Groth16 (requires SP1 Prover Network or CUDA):");
    println!("    SP1_PROVER=network SP1_PRIVATE_KEY=<key> cargo run --release");

    println!("\n=== Done ===");
}
