use eyre::{eyre, Result};
use std::time::Duration;
use tracing::{error, info, warn};

mod prover;
mod state;
mod submitter;
mod torus_rpc;

use prover::BridgeProver;
use state::RelayerState;
use submitter::SepoliaSubmitter;
use torus_rpc::TorusRpcClient;

struct Config {
    torus_rpc_url: String,
    torus_rpc_user: String,
    torus_rpc_pass: String,
    bridge_address: String,
    sepolia_rpc_url: String,
    sepolia_private_key: String,
    wtrs_contract: String,
    poll_interval: Duration,
    min_confirmations: u32,
    state_file: String,
}

impl Config {
    fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let required = |key: &str| -> Result<String> {
            std::env::var(key).map_err(|_| eyre!("missing required env var: {}", key))
        };

        Ok(Self {
            torus_rpc_url: required("TORUS_RPC_URL")?,
            torus_rpc_user: required("TORUS_RPC_USER")?,
            torus_rpc_pass: required("TORUS_RPC_PASS")?,
            bridge_address: required("BRIDGE_ADDRESS")?,
            sepolia_rpc_url: required("SEPOLIA_RPC_URL")?,
            sepolia_private_key: required("SEPOLIA_PRIVATE_KEY")?,
            wtrs_contract: std::env::var("WTRS_CONTRACT")
                .unwrap_or_else(|_| "0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a".to_string()),
            poll_interval: Duration::from_secs(
                std::env::var("POLL_INTERVAL_SECS")
                    .unwrap_or_else(|_| "30".to_string())
                    .parse()?,
            ),
            min_confirmations: std::env::var("MIN_CONFIRMATIONS")
                .unwrap_or_else(|_| "6".to_string())
                .parse()?,
            state_file: std::env::var("STATE_FILE")
                .unwrap_or_else(|_| "relayer_state.json".to_string()),
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config = Config::from_env()?;

    info!("=== Torus Bridge Relayer ===");
    info!("Bridge address:     {}", config.bridge_address);
    info!("Sepolia wTRS:       {}", config.wtrs_contract);
    info!("Poll interval:      {}s", config.poll_interval.as_secs());
    info!("Min confirmations:  {}", config.min_confirmations);

    let rpc = TorusRpcClient::new(
        &config.torus_rpc_url,
        &config.torus_rpc_user,
        &config.torus_rpc_pass,
    );

    let prover = BridgeProver::new()?;

    let submitter = SepoliaSubmitter::new(
        &config.sepolia_rpc_url,
        &config.sepolia_private_key,
        &config.wtrs_contract,
    )?;

    let mut state = RelayerState::load(&config.state_file)?;

    let height = rpc.get_block_count().await?;
    info!("Connected to torus-core at height {}", height);

    info!("Entering main loop...");
    loop {
        match process_cycle(&rpc, &prover, &submitter, &mut state, &config).await {
            Ok(0) => {}
            Ok(n) => info!("Cycle complete: {} deposits processed", n),
            Err(e) => error!("Cycle error: {:#}", e),
        }
        tokio::time::sleep(config.poll_interval).await;
    }
}

async fn process_cycle(
    rpc: &TorusRpcClient,
    prover: &BridgeProver,
    submitter: &SepoliaSubmitter,
    state: &mut RelayerState,
    config: &Config,
) -> Result<usize> {
    let scan = rpc
        .scan_deposits(
            &config.bridge_address,
            state.last_block_hash.as_deref(),
            config.min_confirmations,
        )
        .await?;

    let mut count = 0;

    for deposit in &scan.deposits {
        if state.is_processed(&deposit.txid) {
            continue;
        }

        info!(
            "New deposit: txid={} amount={} TRS confs={}",
            deposit.txid,
            deposit.amount as f64 / 1e8,
            deposit.confirmations,
        );

        let inputs = match rpc.get_proof_inputs(deposit).await {
            Ok(inputs) => inputs,
            Err(e) => {
                warn!("Failed to get proof inputs for {}: {:#}", deposit.txid, e);
                continue;
            }
        };

        info!("Generating ZK proof for {}...", deposit.txid);
        let proof_result = match tokio::task::spawn_blocking({
            let inputs = inputs;
            let elf = prover.elf_clone();
            move || {
                let p = BridgeProver::from_elf(elf);
                p.generate_proof(&inputs)
            }
        })
        .await?
        {
            Ok(result) => result,
            Err(e) => {
                error!("Proof generation failed for {}: {:#}", deposit.txid, e);
                continue;
            }
        };

        let mint_tx = match submitter.submit_mint(&proof_result).await {
            Ok(tx) => tx,
            Err(e) => {
                error!("Mint submission failed for {}: {:#}", deposit.txid, e);
                continue;
            }
        };

        let recipient_hex = format!("0x{}", hex::encode(proof_result.recipient));
        state.mark_processed(
            &deposit.txid,
            &deposit.blockhash,
            deposit.amount,
            &recipient_hex,
            &mint_tx,
        )?;

        info!(
            "Deposit {} minted as wTRS: mint_tx={}",
            deposit.txid, mint_tx
        );
        count += 1;
    }

    state.last_block_hash = Some(scan.lastblock);
    state.save()?;

    Ok(count)
}
