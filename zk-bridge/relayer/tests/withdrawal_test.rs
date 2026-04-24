use torus_bridge_relayer::api::{self, WithdrawalAuth};
use torus_bridge_relayer::covenant_tx::{self, BobAuth, CovenantUtxo, WithdrawalParams};
use torus_bridge_relayer::state::RelayerState;

#[tokio::test]
async fn test_post_and_get_withdrawal_auth() {
    let store = api::new_auth_store();
    api::start_api_server(store.clone(), "127.0.0.1:13001").await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();

    let auth = serde_json::json!({
        "withdrawal_id": "abc123",
        "amount": 1_000_000u64,
        "torus_address": "deadbeef",
        "torus_signature": "sig123",
        "torus_pubkey": "pub456"
    });

    let resp = client
        .post("http://127.0.0.1:13001/api/withdrawal-auth")
        .json(&auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "accepted");
    assert_eq!(body["withdrawal_id"], "abc123");

    let resp = client
        .get("http://127.0.0.1:13001/api/pending-withdrawals")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let pending: Vec<WithdrawalAuth> = resp.json().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].withdrawal_id, "abc123");
    assert_eq!(pending[0].amount, 1_000_000);
}

#[tokio::test]
async fn test_api_rejects_empty_withdrawal_id() {
    let store = api::new_auth_store();
    api::start_api_server(store, "127.0.0.1:13002").await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let client = reqwest::Client::new();
    let auth = serde_json::json!({
        "withdrawal_id": "",
        "amount": 1000u64,
        "torus_address": "addr",
        "torus_signature": "sig",
        "torus_pubkey": "pub"
    });

    let resp = client
        .post("http://127.0.0.1:13002/api/withdrawal-auth")
        .json(&auth)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

#[test]
fn test_covenant_tx_structure() {
    let utxo = CovenantUtxo {
        txid: "a".repeat(64),
        vout: 0,
        script_pubkey: vec![
            0x76, 0xa9, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x88,
            0xac,
        ],
        amount: 100_000_000,
    };

    let withdrawal = WithdrawalParams {
        recipient_addr_hash: [0xab; 20],
        amount: 50_000_000,
    };

    let bob_auth = BobAuth {
        signature: vec![0x30; 72],
        message: b"test-withdrawal".to_vec(),
        pubkey: vec![0x04; 65],
    };

    // secp256k1 private key = 1 (valid for testing)
    let mut key = [0u8; 32];
    key[31] = 1;

    let result = covenant_tx::build_withdrawal_tx(&utxo, &withdrawal, &bob_auth, &[key.to_vec()]);
    assert!(result.is_ok(), "build_withdrawal_tx failed: {:?}", result.err());

    let tx = result.unwrap();
    // version (4) + nTime (4) + at least vin/vout
    assert!(tx.len() > 50);
    assert_eq!(u32::from_le_bytes(tx[0..4].try_into().unwrap()), 1);
}

#[test]
fn test_state_withdrawal_tracking() {
    let path = "/tmp/test_withdrawal_state.json";
    std::fs::remove_file(path).ok();

    let mut state = RelayerState::load(path).unwrap();

    assert_eq!(state.last_withdrawal_block, 0);
    assert!(!state.is_withdrawal_processed("w1"));

    state.mark_withdrawal_processed("w1").unwrap();
    assert!(state.is_withdrawal_processed("w1"));
    assert!(!state.is_withdrawal_processed("w2"));

    state.last_withdrawal_block = 42;
    state.save().unwrap();

    let state2 = RelayerState::load(path).unwrap();
    assert!(state2.is_withdrawal_processed("w1"));
    assert_eq!(state2.last_withdrawal_block, 42);

    std::fs::remove_file(path).ok();
}

#[test]
fn test_state_backward_compatible_deserialization() {
    let path = "/tmp/test_compat_state.json";
    let old_json = r#"{
        "last_block_hash": "abc",
        "processed_deposits": {}
    }"#;
    std::fs::write(path, old_json).unwrap();

    let state = RelayerState::load(path).unwrap();
    assert_eq!(state.last_block_hash, Some("abc".to_string()));
    assert_eq!(state.last_withdrawal_block, 0);
    assert!(state.processed_withdrawals.is_empty());

    std::fs::remove_file(path).ok();
}
