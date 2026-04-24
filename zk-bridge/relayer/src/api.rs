use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::info;

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

async fn post_withdrawal_auth(
    State(store): State<WithdrawalAuthStore>,
    Json(auth): Json<WithdrawalAuth>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if auth.withdrawal_id.is_empty()
        || auth.torus_signature.is_empty()
        || auth.torus_pubkey.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    let withdrawal_id = auth.withdrawal_id.clone();
    store.write().await.insert(withdrawal_id.clone(), auth);

    Ok(Json(serde_json::json!({
        "status": "accepted",
        "withdrawal_id": withdrawal_id,
    })))
}

async fn get_pending_withdrawals(
    State(store): State<WithdrawalAuthStore>,
) -> Json<Vec<WithdrawalAuth>> {
    let map = store.read().await;
    Json(map.values().cloned().collect())
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
