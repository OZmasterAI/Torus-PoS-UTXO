use alloy::primitives::{Address, FixedBytes};
use eyre::{eyre, Result};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use tracing::{error, info, warn};

use torus_bridge_relayer::{api, covenant_tx, prover, state, submitter, torus_rpc, watcher};

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
    bridge_controller_address: String,
    api_bind_addr: String,
    operator_keys: Vec<String>,
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
            bridge_controller_address: std::env::var("BRIDGE_CONTROLLER_ADDRESS")
                .unwrap_or_else(|_| "0x0000000000000000000000000000000000000000".to_string()),
            api_bind_addr: std::env::var("API_BIND_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:3001".to_string()),
            operator_keys: std::env::var("OPERATOR_KEYS")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.trim().to_string())
                .collect(),
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
    info!("BridgeController:   {}", config.bridge_controller_address);
    info!("API bind addr:      {}", config.api_bind_addr);
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
    )?
    .with_bridge_controller(&config.bridge_controller_address)?;

    let mut state = RelayerState::load(&config.state_file)?;

    let auth_store = api::new_auth_store();
    api::start_api_server(auth_store.clone(), &config.api_bind_addr).await;

    let controller_addr = Address::from_str(&config.bridge_controller_address)?;
    let withdrawal_watcher =
        watcher::WithdrawalWatcher::new(&config.sepolia_rpc_url, controller_addr);

    let height = rpc.get_block_count().await?;
    info!("Connected to torus-core at height {}", height);

    info!("Entering main loop...");
    let mut locked_utxos: HashSet<String> = HashSet::new();
    loop {
        locked_utxos.clear();

        match process_cycle(&rpc, &prover, &submitter, &mut state, &config).await {
            Ok(0) => {}
            Ok(n) => info!("Cycle complete: {} deposits processed", n),
            Err(e) => error!("Cycle error: {:#}", e),
        }

        match process_withdrawal_cycle(
            &rpc,
            &prover,
            &submitter,
            &withdrawal_watcher,
            &auth_store,
            &mut state,
            &config,
            &mut locked_utxos,
        )
        .await
        {
            Ok(0) => {}
            Ok(n) => info!("Withdrawal cycle: {} withdrawals processed", n),
            Err(e) => error!("Withdrawal cycle error: {:#}", e),
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

async fn process_withdrawal_cycle(
    rpc: &TorusRpcClient,
    prover: &BridgeProver,
    submitter: &SepoliaSubmitter,
    withdrawal_watcher: &watcher::WithdrawalWatcher,
    auth_store: &api::WithdrawalAuthStore,
    state: &mut RelayerState,
    config: &Config,
    locked_utxos: &mut HashSet<String>,
) -> Result<usize> {
    let (events, last_block) = withdrawal_watcher.watch(state.last_withdrawal_block).await?;
    state.last_withdrawal_block = last_block + 1;

    let mut count = 0;

    for event in &events {
        let withdrawal_id = hex::encode(event.id.as_slice());

        if state.is_withdrawal_processed(&withdrawal_id) {
            continue;
        }

        info!(
            "New withdrawal: id={} amount={} TRS requester={}",
            withdrawal_id,
            event.amount as f64 / 1e8,
            event.requester,
        );

        let bob_auth = {
            let store = auth_store.read().await;
            store.get(&withdrawal_id).cloned()
        };

        let bob_auth = match bob_auth {
            Some(auth) => auth,
            None => {
                info!("No Bob authorization for withdrawal {}, skipping", withdrawal_id);
                continue;
            }
        };

        let utxos = rpc.list_unspent(&config.bridge_address).await?;
        let utxo_data = utxos.iter().find(|u| {
            let outpoint = format!(
                "{}:{}",
                u["txid"].as_str().unwrap_or_default(),
                u["vout"].as_u64().unwrap_or(0)
            );
            if locked_utxos.contains(&outpoint) {
                return false;
            }
            let amt = (u["amount"].as_f64().unwrap_or(0.0) * 1e8) as u64;
            amt >= event.amount
        });

        let utxo_data = match utxo_data {
            Some(u) => u,
            None => {
                warn!("No suitable UTXO for withdrawal {}", withdrawal_id);
                continue;
            }
        };

        let outpoint = format!(
            "{}:{}",
            utxo_data["txid"].as_str().unwrap_or_default(),
            utxo_data["vout"].as_u64().unwrap_or(0)
        );
        locked_utxos.insert(outpoint);

        let utxo = covenant_tx::CovenantUtxo {
            txid: utxo_data["txid"].as_str().unwrap_or_default().to_string(),
            vout: utxo_data["vout"].as_u64().unwrap_or(0) as u32,
            script_pubkey: hex::decode(
                utxo_data["scriptPubKey"].as_str().unwrap_or_default(),
            )
            .unwrap_or_default(),
            amount: (utxo_data["amount"].as_f64().unwrap_or(0.0) * 1e8) as u64,
        };

        let withdrawal_params = covenant_tx::WithdrawalParams {
            recipient_addr_hash: event.torus_address,
            amount: event.amount,
        };

        let canonical_msg = format!(
            "{}:{}:{}",
            withdrawal_id, event.amount, bob_auth.torus_address
        );
        let bob = covenant_tx::BobAuth {
            signature: hex::decode(&bob_auth.torus_signature).unwrap_or_default(),
            message: canonical_msg.into_bytes(),
            pubkey: hex::decode(&bob_auth.torus_pubkey).unwrap_or_default(),
        };

        let operator_keys: Vec<Vec<u8>> = config
            .operator_keys
            .iter()
            .filter_map(|k| hex::decode(k.strip_prefix("0x").unwrap_or(k)).ok())
            .collect();

        let raw_tx = match covenant_tx::build_withdrawal_tx(
            &utxo,
            &withdrawal_params,
            &bob,
            &operator_keys,
        ) {
            Ok(tx) => tx,
            Err(e) => {
                error!("Failed to build covenant tx for {}: {:#}", withdrawal_id, e);
                continue;
            }
        };

        let torus_txid = match covenant_tx::broadcast_withdrawal_tx(rpc, &raw_tx).await {
            Ok(txid) => txid,
            Err(e) => {
                error!("Failed to broadcast covenant tx for {}: {:#}", withdrawal_id, e);
                continue;
            }
        };

        info!("Covenant tx broadcast: {} for withdrawal {}", torus_txid, withdrawal_id);

        let mut confirmed = false;
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_secs(15)).await;
            match rpc.get_transaction(&torus_txid).await {
                Ok(tx) => {
                    let confs = tx["confirmations"].as_u64().unwrap_or(0);
                    if confs >= config.min_confirmations as u64 {
                        confirmed = true;
                        break;
                    }
                    info!("Waiting for confirmations: {}/{}", confs, config.min_confirmations);
                }
                Err(_) => continue,
            }
        }

        if !confirmed {
            warn!("Timed out waiting for torus-core confirmation for {}", torus_txid);
            continue;
        }

        let tx_data = rpc.get_transaction(&torus_txid).await?;
        let block_hash = tx_data["blockhash"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let deposit_info = torus_rpc::DepositInfo {
            txid: torus_txid.clone(),
            blockhash: block_hash,
            amount: event.amount,
            confirmations: config.min_confirmations,
        };

        let inputs = match rpc.get_proof_inputs(&deposit_info).await {
            Ok(inputs) => inputs,
            Err(e) => {
                error!("Failed to get proof inputs for withdrawal {}: {:#}", withdrawal_id, e);
                continue;
            }
        };

        info!("Generating withdrawal ZK proof for {}...", withdrawal_id);
        let proof_result = match tokio::task::spawn_blocking({
            let inputs = inputs;
            let elf = prover.elf_clone();
            move || {
                let p = BridgeProver::from_elf(elf);
                p.generate_withdrawal_proof(&inputs)
            }
        })
        .await?
        {
            Ok(result) => result,
            Err(e) => {
                error!("Withdrawal proof failed for {}: {:#}", withdrawal_id, e);
                continue;
            }
        };

        match submitter
            .confirm_withdrawal(
                event.id,
                proof_result.calldata.clone(),
                FixedBytes::from(proof_result.block_hash_be),
                FixedBytes::from(proof_result.tx_hash_be),
            )
            .await
        {
            Ok(confirm_tx) => {
                info!("Withdrawal {} confirmed on Sepolia: {}", withdrawal_id, confirm_tx);
            }
            Err(e) => {
                error!("Failed to confirm withdrawal {} on Sepolia: {:#}", withdrawal_id, e);
                continue;
            }
        }

        state.mark_withdrawal_processed(&withdrawal_id)?;

        {
            let mut store = auth_store.write().await;
            store.remove(&withdrawal_id);
        }

        count += 1;
    }

    state.save()?;
    Ok(count)
}
