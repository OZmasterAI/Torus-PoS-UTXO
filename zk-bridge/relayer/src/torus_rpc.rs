use eyre::{bail, eyre, Result};
use reqwest::Client;
use serde_json::{json, Value};
use torus_kernel::{double_sha256, BlockHeader, StakeKernelInput};

pub struct TorusRpcClient {
    url: String,
    client: Client,
    user: String,
    pass: String,
}

#[derive(Debug, Clone)]
pub struct DepositInfo {
    pub txid: String,
    pub blockhash: String,
    pub amount: u64,
    pub confirmations: u32,
}

#[derive(Debug)]
pub struct ScanResult {
    pub deposits: Vec<DepositInfo>,
    pub lastblock: String,
}

#[derive(Debug)]
pub struct ProofInputs {
    pub header: BlockHeader,
    pub kernel_input: StakeKernelInput,
    pub deposit_tx_hash: [u8; 32],
    pub merkle_proof: Vec<([u8; 32], bool)>,
    pub amount: u64,
    pub recipient: [u8; 20],
}

impl TorusRpcClient {
    pub fn new(url: &str, user: &str, pass: &str) -> Self {
        Self {
            url: url.to_string(),
            client: Client::new(),
            user: user.to_string(),
            pass: pass.to_string(),
        }
    }

    async fn call(&self, method: &str, params: &[Value]) -> Result<Value> {
        let body = json!({
            "jsonrpc": "1.0",
            "id": "relayer",
            "method": method,
            "params": params,
        });

        let resp = self
            .client
            .post(&self.url)
            .basic_auth(&self.user, Some(&self.pass))
            .json(&body)
            .send()
            .await?
            .json::<Value>()
            .await?;

        if let Some(err) = resp.get("error") {
            if !err.is_null() {
                bail!("RPC error ({}): {}", method, err);
            }
        }

        resp.get("result")
            .cloned()
            .ok_or_else(|| eyre!("missing result in RPC response"))
    }

    pub async fn get_block_count(&self) -> Result<u64> {
        let result = self.call("getblockcount", &[]).await?;
        result
            .as_u64()
            .ok_or_else(|| eyre!("invalid block count"))
    }

    pub async fn get_block_hash(&self, height: u64) -> Result<String> {
        let result = self.call("getblockhash", &[json!(height)]).await?;
        Ok(result
            .as_str()
            .ok_or_else(|| eyre!("invalid block hash"))?
            .to_string())
    }

    async fn get_block_verbose(&self, hash: &str) -> Result<Value> {
        self.call("getblock", &[json!(hash), json!(true)]).await
    }

    async fn get_raw_tx_verbose(&self, txid: &str) -> Result<Value> {
        self.call("getrawtransaction", &[json!(txid), json!(1)])
            .await
    }

    pub async fn scan_deposits(
        &self,
        bridge_address: &str,
        since_block: Option<&str>,
        min_confirmations: u32,
    ) -> Result<ScanResult> {
        let params: Vec<Value> = match since_block {
            Some(hash) => vec![json!(hash), json!(min_confirmations)],
            None => {
                let genesis = self.get_block_hash(0).await?;
                vec![json!(genesis), json!(min_confirmations)]
            }
        };

        let result = self.call("listsinceblock", &params).await?;

        let lastblock = result["lastblock"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        let txs = result["transactions"]
            .as_array()
            .ok_or_else(|| eyre!("missing transactions array"))?;

        let mut deposits = Vec::new();
        for tx in txs {
            let category = tx["category"].as_str().unwrap_or_default();
            let address = tx["address"].as_str().unwrap_or_default();

            if category == "receive" && address == bridge_address {
                let amount_f64 = tx["amount"].as_f64().unwrap_or(0.0);
                let amount = (amount_f64 * 1e8).round() as u64;

                deposits.push(DepositInfo {
                    txid: tx["txid"].as_str().unwrap_or_default().to_string(),
                    blockhash: tx["blockhash"].as_str().unwrap_or_default().to_string(),
                    amount,
                    confirmations: tx["confirmations"].as_u64().unwrap_or(0) as u32,
                });
            }
        }

        Ok(ScanResult { deposits, lastblock })
    }

    pub async fn get_proof_inputs(&self, deposit: &DepositInfo) -> Result<ProofInputs> {
        let block = self.get_block_verbose(&deposit.blockhash).await?;

        let header = parse_block_header(&block)?;
        let kernel_input = self.parse_kernel_input(&block).await?;

        let tx_list = block["tx"]
            .as_array()
            .ok_or_else(|| eyre!("missing tx list in block"))?;

        let tx_hashes: Vec<[u8; 32]> = tx_list
            .iter()
            .map(|t| hex_to_internal(t.as_str().unwrap_or_default()))
            .collect();

        let deposit_tx_hash = hex_to_internal(&deposit.txid);
        let target_idx = tx_hashes
            .iter()
            .position(|h| h == &deposit_tx_hash)
            .ok_or_else(|| eyre!("deposit tx {} not found in block", deposit.txid))?;

        let merkle_proof = build_merkle_proof(&tx_hashes, target_idx);

        let recipient = self.extract_recipient(&deposit.txid).await?;

        Ok(ProofInputs {
            header,
            kernel_input,
            deposit_tx_hash,
            merkle_proof,
            amount: deposit.amount,
            recipient,
        })
    }

    pub async fn send_raw_transaction(&self, hex_tx: &str) -> Result<String> {
        let result = self.call("sendrawtransaction", &[json!(hex_tx)]).await?;
        Ok(result.as_str().unwrap_or_default().to_string())
    }

    pub async fn get_transaction(&self, txid: &str) -> Result<Value> {
        self.call("gettransaction", &[json!(txid)]).await
    }

    pub async fn list_unspent(&self, address: &str) -> Result<Vec<Value>> {
        let result = self.call("listunspent", &[json!(1), json!(9999999), json!([address])]).await?;
        result.as_array().cloned().ok_or_else(|| eyre!("invalid listunspent response"))
    }

    async fn parse_kernel_input(&self, block: &Value) -> Result<StakeKernelInput> {
        let n_bits =
            u32::from_str_radix(block["bits"].as_str().unwrap_or("0"), 16)?;

        let stake_modifier = u64::from_str_radix(
            block["modifier"]
                .as_str()
                .or_else(|| block["stakemodifier"].as_str())
                .unwrap_or("0"),
            16,
        )?;

        let block_time = block["time"].as_u64().unwrap_or(0) as u32;

        // Coinstake is the second transaction in a PoS block
        let tx_list = block["tx"]
            .as_array()
            .ok_or_else(|| eyre!("missing tx list"))?;

        if tx_list.len() < 2 {
            bail!("block has no coinstake transaction (not a PoS block?)");
        }

        let coinstake_txid = tx_list[1]
            .as_str()
            .ok_or_else(|| eyre!("invalid coinstake txid"))?;
        let coinstake = self.get_raw_tx_verbose(coinstake_txid).await?;

        let time_tx = coinstake["time"].as_u64().unwrap_or(block_time as u64) as u32;

        let vin = coinstake["vin"]
            .as_array()
            .ok_or_else(|| eyre!("missing vin in coinstake"))?;
        if vin.is_empty() {
            bail!("coinstake has no inputs");
        }

        let prev_txid = vin[0]["txid"]
            .as_str()
            .ok_or_else(|| eyre!("missing prev txid in coinstake input"))?;
        let prevout_n = vin[0]["vout"].as_u64().unwrap_or(0) as u32;

        let prev_tx = self.get_raw_tx_verbose(prev_txid).await?;
        let tx_prev_time = prev_tx["time"].as_u64().unwrap_or(0) as u32;

        let value_in = prev_tx["vout"]
            .as_array()
            .and_then(|vouts| vouts.get(prevout_n as usize))
            .and_then(|v| v["value"].as_f64())
            .map(|v| (v * 1e8).round() as u64)
            .unwrap_or(0);

        // Get the block containing the prev tx for block_time_from
        let prev_blockhash = prev_tx["blockhash"].as_str().unwrap_or_default();
        let block_time_from = if !prev_blockhash.is_empty() {
            let prev_block = self.get_block_verbose(prev_blockhash).await?;
            prev_block["time"].as_u64().unwrap_or(0) as u32
        } else {
            tx_prev_time
        };

        // tx_prev_offset: byte offset of the prev tx within its block.
        // Requires raw block parsing. Falls back to 0 if unavailable.
        let tx_prev_offset = self
            .get_tx_offset_in_block(prev_txid, prev_blockhash)
            .await
            .unwrap_or(0);

        Ok(StakeKernelInput {
            n_bits,
            stake_modifier,
            block_time_from,
            tx_prev_offset,
            tx_prev_time,
            prevout_n,
            time_tx,
            value_in,
            is_permanent_stake: false,
        })
    }

    async fn get_tx_offset_in_block(&self, txid: &str, blockhash: &str) -> Result<u32> {
        if blockhash.is_empty() {
            bail!("no block hash for tx offset calculation");
        }

        let raw_hex = self
            .call("getblock", &[json!(blockhash), json!(false)])
            .await?;
        let raw_hex = raw_hex
            .as_str()
            .ok_or_else(|| eyre!("raw block not a string"))?;
        let block_bytes = hex::decode(raw_hex)?;

        // Block: header (80 bytes) + tx_count (varint) + transactions
        let mut offset = 80usize;
        let (tx_count, vs) = read_varint(&block_bytes[offset..])?;
        offset += vs;

        let target_hash = hex_to_internal(txid);

        for _ in 0..tx_count {
            let tx_start = offset;
            let tx_size = measure_ppc_transaction(&block_bytes[offset..])?;
            let tx_bytes = &block_bytes[offset..offset + tx_size];
            let computed_hash = double_sha256(tx_bytes);

            if computed_hash == target_hash {
                return Ok(tx_start as u32);
            }
            offset += tx_size;
        }

        bail!("transaction {} not found in raw block {}", txid, blockhash)
    }

    async fn extract_recipient(&self, txid: &str) -> Result<[u8; 20]> {
        let tx = self.get_raw_tx_verbose(txid).await?;

        let vouts = tx["vout"]
            .as_array()
            .ok_or_else(|| eyre!("missing vout in transaction"))?;

        for vout in vouts {
            let script_type = vout["scriptPubKey"]["type"]
                .as_str()
                .unwrap_or_default();
            if script_type == "nulldata" {
                let hex_data = vout["scriptPubKey"]["hex"]
                    .as_str()
                    .ok_or_else(|| eyre!("missing script hex in OP_RETURN output"))?;
                let script_bytes = hex::decode(hex_data)?;

                // OP_RETURN (0x6a) + PUSH_20 (0x14) + 20-byte EVM address
                if script_bytes.len() >= 22 && script_bytes[0] == 0x6a && script_bytes[1] == 0x14 {
                    let mut recipient = [0u8; 20];
                    recipient.copy_from_slice(&script_bytes[2..22]);
                    return Ok(recipient);
                }
            }
        }

        bail!("no OP_RETURN with EVM recipient found in tx {}", txid)
    }
}

fn parse_block_header(block: &Value) -> Result<BlockHeader> {
    Ok(BlockHeader {
        version: block["version"].as_u64().unwrap_or(1) as u32,
        hash_prev_block: hex_to_internal(
            block["previousblockhash"]
                .as_str()
                .unwrap_or(&"0".repeat(64)),
        ),
        hash_merkle_root: hex_to_internal(
            block["merkleroot"]
                .as_str()
                .ok_or_else(|| eyre!("missing merkleroot"))?,
        ),
        time: block["time"].as_u64().unwrap_or(0) as u32,
        bits: u32::from_str_radix(block["bits"].as_str().unwrap_or("0"), 16)?,
        nonce: block["nonce"].as_u64().unwrap_or(0) as u32,
    })
}

/// Convert RPC display hex (big-endian) to internal byte order (little-endian).
pub fn hex_to_internal(hex_str: &str) -> [u8; 32] {
    let bytes = hex::decode(hex_str).unwrap_or_else(|_| vec![0u8; 32]);
    let mut result = [0u8; 32];
    let len = bytes.len().min(32);
    for i in 0..len {
        result[i] = bytes[len - 1 - i];
    }
    result
}

/// Build a Bitcoin-style Merkle inclusion proof for a transaction at `target_idx`.
/// Returns sibling hashes with direction flags matching `torus_kernel::verify_merkle_proof`.
pub fn build_merkle_proof(tx_hashes: &[[u8; 32]], target_idx: usize) -> Vec<([u8; 32], bool)> {
    let mut proof = Vec::new();
    let mut level: Vec<[u8; 32]> = tx_hashes.to_vec();
    let mut idx = target_idx;

    while level.len() > 1 {
        if level.len() % 2 == 1 {
            let last = *level.last().unwrap();
            level.push(last);
        }

        let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        let is_right = idx % 2 == 0;
        proof.push((level[sibling_idx], is_right));

        let mut next_level = Vec::new();
        for pair in level.chunks(2) {
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(&pair[0]);
            combined.extend_from_slice(&pair[1]);
            next_level.push(double_sha256(&combined));
        }

        level = next_level;
        idx /= 2;
    }

    proof
}

fn read_varint(data: &[u8]) -> Result<(u64, usize)> {
    if data.is_empty() {
        bail!("empty data for varint");
    }
    match data[0] {
        0..=0xfc => Ok((data[0] as u64, 1)),
        0xfd => {
            if data.len() < 3 {
                bail!("truncated varint");
            }
            Ok((u16::from_le_bytes([data[1], data[2]]) as u64, 3))
        }
        0xfe => {
            if data.len() < 5 {
                bail!("truncated varint");
            }
            Ok((
                u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as u64,
                5,
            ))
        }
        0xff => {
            if data.len() < 9 {
                bail!("truncated varint");
            }
            Ok((u64::from_le_bytes(data[1..9].try_into()?), 9))
        }
    }
}

/// Measure the byte size of a serialized PPCoin transaction.
/// Format: version(4) + nTime(4) + vin_count(var) + vins + vout_count(var) + vouts + locktime(4)
fn measure_ppc_transaction(data: &[u8]) -> Result<usize> {
    let mut pos = 0;

    pos += 4; // version
    pos += 4; // nTime (PPCoin extension)

    let (vin_count, vs) = read_varint(&data[pos..])?;
    pos += vs;
    for _ in 0..vin_count {
        pos += 32; // prev txid
        pos += 4; // prev vout
        let (script_len, vs) = read_varint(&data[pos..])?;
        pos += vs;
        pos += script_len as usize;
        pos += 4; // sequence
    }

    let (vout_count, vs) = read_varint(&data[pos..])?;
    pos += vs;
    for _ in 0..vout_count {
        pos += 8; // value
        let (script_len, vs) = read_varint(&data[pos..])?;
        pos += vs;
        pos += script_len as usize;
    }

    pos += 4; // lock_time

    Ok(pos)
}
