use crate::torus_rpc::{hex_to_internal, TorusRpcClient};
use eyre::{eyre, Result};
use torus_kernel::double_sha256;
use tracing::info;

#[derive(Debug, Clone)]
pub struct CovenantUtxo {
    pub txid: String,
    pub vout: u32,
    pub script_pubkey: Vec<u8>,
    pub amount: u64,
}

#[derive(Debug, Clone)]
pub struct WithdrawalParams {
    pub recipient_addr_hash: [u8; 20],
    pub amount: u64,
}

#[derive(Debug, Clone)]
pub struct BobAuth {
    pub signature: Vec<u8>,
    pub message: Vec<u8>,
    pub pubkey: Vec<u8>,
}

pub fn build_withdrawal_tx(
    utxo: &CovenantUtxo,
    withdrawal: &WithdrawalParams,
    bob_auth: &BobAuth,
    operator_keys: &[Vec<u8>],
) -> Result<Vec<u8>> {
    let mut unsigned_tx = build_unsigned_tx(utxo, withdrawal);

    let sighash = compute_sighash(&unsigned_tx, &utxo.script_pubkey);

    let operator_sigs = sign_all(operator_keys, &sighash)?;

    let script_sig = build_path_a_script_sig(
        &withdrawal.recipient_addr_hash,
        withdrawal.amount,
        bob_auth,
        &operator_sigs,
    );

    set_script_sig(&mut unsigned_tx, utxo, &script_sig);

    Ok(unsigned_tx)
}

fn build_unsigned_tx(utxo: &CovenantUtxo, withdrawal: &WithdrawalParams) -> Vec<u8> {
    let mut tx = Vec::new();

    // version (4 bytes LE)
    tx.extend_from_slice(&1u32.to_le_bytes());
    // nTime (4 bytes LE) — PPC extension
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    tx.extend_from_slice(&now.to_le_bytes());

    // vin count
    tx.push(1u8);
    // prev txid (internal byte order)
    tx.extend_from_slice(&hex_to_internal(&utxo.txid));
    // prev vout
    tx.extend_from_slice(&utxo.vout.to_le_bytes());
    // scriptSig (empty for unsigned)
    tx.push(0u8);
    // sequence
    tx.extend_from_slice(&0xffffffffu32.to_le_bytes());

    // vout count
    tx.push(1u8);
    // value
    tx.extend_from_slice(&withdrawal.amount.to_le_bytes());
    // scriptPubKey: P2PKH to recipient
    let spk = build_p2pkh_script(&withdrawal.recipient_addr_hash);
    push_varint(&mut tx, spk.len() as u64);
    tx.extend_from_slice(&spk);

    // locktime
    tx.extend_from_slice(&0u32.to_le_bytes());

    tx
}

fn compute_sighash(unsigned_tx: &[u8], script_pubkey: &[u8]) -> [u8; 32] {
    // Replace empty scriptSig with scriptPubKey for signing
    // The unsigned tx has: ...prev_vout(4) + scriptSig_len(1=0x00) + sequence(4)...
    // We need to find the scriptSig position and replace it

    let mut tx_copy = Vec::new();

    // version (4) + nTime (4) + vin_count (1) + prev_txid (32) + prev_vout (4) = 45 bytes
    tx_copy.extend_from_slice(&unsigned_tx[..45]);
    // Replace scriptSig with scriptPubKey
    push_varint(&mut tx_copy, script_pubkey.len() as u64);
    tx_copy.extend_from_slice(script_pubkey);
    // Skip the empty scriptSig (1 byte: 0x00), continue from sequence
    tx_copy.extend_from_slice(&unsigned_tx[46..]);

    // Append SIGHASH_ALL
    tx_copy.extend_from_slice(&1u32.to_le_bytes());

    double_sha256(&tx_copy)
}

fn sign_all(operator_keys: &[Vec<u8>], sighash: &[u8; 32]) -> Result<Vec<Vec<u8>>> {
    use k256::ecdsa::{SigningKey, Signature};
    use k256::ecdsa::signature::hazmat::PrehashSigner;

    let mut sigs = Vec::new();
    for key_bytes in operator_keys {
        let signing_key = SigningKey::from_slice(key_bytes)
            .map_err(|e| eyre!("invalid operator key: {}", e))?;
        let (sig, _): (Signature, _) = signing_key.sign_prehash(sighash)
            .map_err(|e| eyre!("signing failed: {}", e))?;
        let mut der = sig.to_der().as_bytes().to_vec();
        der.push(0x01); // SIGHASH_ALL
        sigs.push(der);
    }
    Ok(sigs)
}

fn set_script_sig(tx: &mut Vec<u8>, _utxo: &CovenantUtxo, script_sig: &[u8]) {
    // Rebuild the tx with the actual scriptSig
    let mut new_tx = Vec::new();

    // version (4) + nTime (4) + vin_count (1) + prev_txid (32) + prev_vout (4) = 45 bytes
    new_tx.extend_from_slice(&tx[..45]);
    // Write actual scriptSig
    push_varint(&mut new_tx, script_sig.len() as u64);
    new_tx.extend_from_slice(script_sig);
    // Skip old empty scriptSig (1 byte: 0x00), copy from sequence onward
    new_tx.extend_from_slice(&tx[46..]);

    *tx = new_tx;
}

fn encode_script_num(val: u64) -> Vec<u8> {
    if val == 0 {
        return vec![];
    }
    let mut bytes = val.to_le_bytes().to_vec();
    while bytes.last() == Some(&0) {
        bytes.pop();
    }
    if bytes.last().map_or(false, |b| b & 0x80 != 0) {
        bytes.push(0x00);
    }
    bytes
}

fn build_path_a_script_sig(
    addr_hash: &[u8; 20],
    amount: u64,
    bob_auth: &BobAuth,
    operator_sigs: &[Vec<u8>],
) -> Vec<u8> {
    let mut script = Vec::new();

    push_data(&mut script, addr_hash);
    push_data(&mut script, &encode_script_num(amount));
    push_data(&mut script, &bob_auth.signature);
    push_data(&mut script, &bob_auth.message);
    push_data(&mut script, &bob_auth.pubkey);
    for sig in operator_sigs {
        push_data(&mut script, sig);
    }
    script.push(0x00); // OP_0
    script.push(0x51); // OP_TRUE (OP_1)

    script
}

fn build_p2pkh_script(pubkey_hash: &[u8; 20]) -> Vec<u8> {
    let mut script = Vec::new();
    script.push(0x76); // OP_DUP
    script.push(0xa9); // OP_HASH160
    script.push(0x14); // push 20 bytes
    script.extend_from_slice(pubkey_hash);
    script.push(0x88); // OP_EQUALVERIFY
    script.push(0xac); // OP_CHECKSIG
    script
}

fn push_data(script: &mut Vec<u8>, data: &[u8]) {
    let len = data.len();
    if len < 0x4c {
        script.push(len as u8);
    } else if len <= 0xff {
        script.push(0x4c); // OP_PUSHDATA1
        script.push(len as u8);
    } else if len <= 0xffff {
        script.push(0x4d); // OP_PUSHDATA2
        script.extend_from_slice(&(len as u16).to_le_bytes());
    }
    script.extend_from_slice(data);
}

fn push_varint(buf: &mut Vec<u8>, val: u64) {
    if val < 0xfd {
        buf.push(val as u8);
    } else if val <= 0xffff {
        buf.push(0xfd);
        buf.extend_from_slice(&(val as u16).to_le_bytes());
    } else if val <= 0xffffffff {
        buf.push(0xfe);
        buf.extend_from_slice(&(val as u32).to_le_bytes());
    } else {
        buf.push(0xff);
        buf.extend_from_slice(&val.to_le_bytes());
    }
}

pub async fn broadcast_withdrawal_tx(
    rpc: &TorusRpcClient,
    raw_tx: &[u8],
) -> Result<String> {
    let tx_hex = hex::encode(raw_tx);
    info!("Broadcasting withdrawal tx ({} bytes)...", raw_tx.len());
    rpc.send_raw_transaction(&tx_hex).await
}
