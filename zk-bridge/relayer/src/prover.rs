use crate::torus_rpc::ProofInputs;
use eyre::Result;
use sp1_sdk::blocking::{Elf, ProveRequest, Prover, ProverClient, SP1Stdin};
use sp1_sdk::ProvingKey;
use tracing::info;

pub struct BridgeProver {
    elf: Elf,
}

pub struct ProofResult {
    /// Concatenated [publicValues (160 bytes) | groth16Proof] for WrappedTRS.mint()
    pub calldata: Vec<u8>,
    pub block_hash_be: [u8; 32],
    pub tx_hash_be: [u8; 32],
    pub amount: u64,
    pub recipient: [u8; 20],
}

impl BridgeProver {
    pub fn new() -> Result<Self> {
        let elf_bytes = include_bytes!("../../program/elf/riscv32im-succinct-zkvm-elf");
        let elf: Elf = elf_bytes.as_slice().into();
        Ok(Self { elf })
    }

    pub fn from_elf(elf: Elf) -> Self {
        Self { elf }
    }

    pub fn elf_clone(&self) -> Elf {
        self.elf.clone()
    }

    pub fn generate_proof(&self, inputs: &ProofInputs) -> Result<ProofResult> {
        let mut stdin = SP1Stdin::new();
        stdin.write(&0u8);
        stdin.write(&inputs.header);
        stdin.write(&inputs.kernel_input);
        stdin.write(&inputs.deposit_tx_hash);
        stdin.write(&inputs.merkle_proof);
        stdin.write(&inputs.amount);
        stdin.write(&inputs.recipient);

        info!("Connecting to SP1 Prover Network...");

        #[cfg(feature = "network")]
        let client = ProverClient::builder().network().build();
        #[cfg(not(feature = "network"))]
        let client = ProverClient::builder().cpu().build();

        info!("Setting up proving key...");
        let pk = client.setup(self.elf.clone()).expect("SP1 setup failed");
        let vk = pk.verifying_key().clone();

        info!("Generating Groth16 proof (this may take ~35s on Prover Network)...");
        let t0 = std::time::Instant::now();
        let proof = client
            .prove(&pk, stdin)
            .groth16()
            .run()
            .expect("Groth16 proving failed");
        info!("Proof generated in {:.1?}", t0.elapsed());

        info!("Verifying proof locally...");
        client
            .verify(&proof, &vk, None)
            .expect("proof verification failed");

        let public_values = proof.public_values.as_slice();
        let proof_bytes = proof.bytes();

        assert!(
            public_values.len() >= 160,
            "expected 160 bytes of public values, got {}",
            public_values.len()
        );

        let mut block_hash_be = [0u8; 32];
        let mut tx_hash_be = [0u8; 32];
        block_hash_be.copy_from_slice(&public_values[0..32]);
        tx_hash_be.copy_from_slice(&public_values[64..96]);

        let mut calldata = Vec::with_capacity(public_values.len() + proof_bytes.len());
        calldata.extend_from_slice(public_values);
        calldata.extend_from_slice(&proof_bytes);

        info!(
            "Calldata ready: {} bytes ({} public values + {} proof)",
            calldata.len(),
            public_values.len(),
            proof_bytes.len()
        );

        Ok(ProofResult {
            calldata,
            block_hash_be,
            tx_hash_be,
            amount: inputs.amount,
            recipient: inputs.recipient,
        })
    }

    pub fn generate_withdrawal_proof(&self, inputs: &ProofInputs) -> Result<ProofResult> {
        let mut stdin = SP1Stdin::new();
        stdin.write(&1u8);
        stdin.write(&inputs.header);
        stdin.write(&inputs.kernel_input);
        stdin.write(&inputs.deposit_tx_hash);
        stdin.write(&inputs.merkle_proof);
        stdin.write(&inputs.amount);
        stdin.write(&inputs.recipient);

        info!("Connecting to SP1 Prover Network...");

        #[cfg(feature = "network")]
        let client = ProverClient::builder().network().build();
        #[cfg(not(feature = "network"))]
        let client = ProverClient::builder().cpu().build();

        info!("Setting up proving key...");
        let pk = client.setup(self.elf.clone()).expect("SP1 setup failed");
        let vk = pk.verifying_key().clone();

        info!("Generating Groth16 withdrawal proof (this may take ~35s on Prover Network)...");
        let t0 = std::time::Instant::now();
        let proof = client
            .prove(&pk, stdin)
            .groth16()
            .run()
            .expect("Groth16 proving failed");
        info!("Withdrawal proof generated in {:.1?}", t0.elapsed());

        info!("Verifying proof locally...");
        client
            .verify(&proof, &vk, None)
            .expect("proof verification failed");

        let public_values = proof.public_values.as_slice();
        let proof_bytes = proof.bytes();

        assert!(
            public_values.len() >= 160,
            "expected 160 bytes of public values, got {}",
            public_values.len()
        );

        let mut block_hash_be = [0u8; 32];
        let mut tx_hash_be = [0u8; 32];
        block_hash_be.copy_from_slice(&public_values[0..32]);
        tx_hash_be.copy_from_slice(&public_values[64..96]);

        let mut calldata = Vec::with_capacity(public_values.len() + proof_bytes.len());
        calldata.extend_from_slice(public_values);
        calldata.extend_from_slice(&proof_bytes);

        info!(
            "Calldata ready: {} bytes ({} public values + {} proof)",
            calldata.len(),
            public_values.len(),
            proof_bytes.len()
        );

        Ok(ProofResult {
            calldata,
            block_hash_be,
            tx_hash_be,
            amount: inputs.amount,
            recipient: inputs.recipient,
        })
    }
}
