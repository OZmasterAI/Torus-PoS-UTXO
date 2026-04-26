use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WithdrawalAuth {
    pub withdrawal_id: String,
    pub amount: u64,
    pub torus_address: String,
    pub torus_signature: String,
    pub torus_pubkey: String,
}

pub type WithdrawalAuthStore = Arc<RwLock<HashMap<String, WithdrawalAuth>>>;

pub fn new_auth_store() -> WithdrawalAuthStore {
    Arc::new(RwLock::new(HashMap::new()))
}

fn verify_withdrawal_signature(auth: &WithdrawalAuth) -> bool {
    let message = format!(
        "{}:{}:{}",
        auth.withdrawal_id, auth.amount, auth.torus_address
    );

    let sig_bytes = match hex::decode(auth.torus_signature.trim_start_matches("0x")) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let pubkey_bytes = match hex::decode(auth.torus_pubkey.trim_start_matches("0x")) {
        Ok(b) => b,
        Err(_) => return false,
    };

    match secp256k1::Secp256k1::verification_only()
        .verify_ecdsa(
            &secp256k1::Message::from_digest(
                bitcoin_hashes::sha256d::Hash::hash(message.as_bytes()).to_byte_array(),
            ),
            &match secp256k1::ecdsa::Signature::from_compact(&sig_bytes) {
                Ok(s) => s,
                Err(_) => return false,
            },
            &match secp256k1::PublicKey::from_slice(&pubkey_bytes) {
                Ok(k) => k,
                Err(_) => return false,
            },
        ) {
        Ok(()) => true,
        Err(_) => false,
    }
}

const MAX_STORE_SIZE: usize = 10_000;
const MAX_WITHDRAWAL_ID_LEN: usize = 66;

async fn post_withdrawal_auth(
    State(store): State<WithdrawalAuthStore>,
    Json(auth): Json<WithdrawalAuth>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if auth.withdrawal_id.is_empty()
        || auth.withdrawal_id.len() > MAX_WITHDRAWAL_ID_LEN
        || auth.torus_signature.is_empty()
        || auth.torus_pubkey.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    if !verify_withdrawal_signature(&auth) {
        warn!("withdrawal auth rejected: invalid signature for {}", auth.withdrawal_id);
        return Err(StatusCode::UNAUTHORIZED);
    }

    let withdrawal_id = auth.withdrawal_id.clone();
    let mut map = store.write().await;
    if map.len() >= MAX_STORE_SIZE && !map.contains_key(&withdrawal_id) {
        warn!("withdrawal auth store full ({MAX_STORE_SIZE}), rejecting {withdrawal_id}");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    map.insert(withdrawal_id.clone(), auth);

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "withdrawal_id": withdrawal_id,
    })))
}

#[derive(Serialize)]
struct PendingWithdrawalPublic {
    withdrawal_id: String,
    amount: u64,
    torus_address: String,
}

async fn get_pending_withdrawals(
    headers: HeaderMap,
    State(store): State<WithdrawalAuthStore>,
) -> Result<Json<Vec<PendingWithdrawalPublic>>, StatusCode> {
    let api_key = std::env::var("RELAYER_API_KEY").unwrap_or_default();
    if api_key.is_empty() {
        warn!("RELAYER_API_KEY not set — pending-withdrawals endpoint disabled");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if provided != api_key {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let map = store.read().await;
    let public: Vec<PendingWithdrawalPublic> = map
        .values()
        .map(|a| PendingWithdrawalPublic {
            withdrawal_id: a.withdrawal_id.clone(),
            amount: a.amount,
            torus_address: a.torus_address.clone(),
        })
        .collect();
    Ok(Json(public))
}

pub async fn start_api_server(store: WithdrawalAuthStore, bind_addr: &str) {
    let app = Router::new()
        .route("/api/withdrawal-auth", post(post_withdrawal_auth))
        .route("/api/pending-withdrawals", get(get_pending_withdrawals))
        .with_state(store);

    let addr: SocketAddr = bind_addr.parse().expect("invalid bind address");
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind API listener");

    info!("API server listening on {addr}");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("API server error");
    });
}
