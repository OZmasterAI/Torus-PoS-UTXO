use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::Filter;
use alloy::sol_types::SolEvent;
use eyre::{eyre, Result};
use tracing::info;

alloy::sol! {
    event WithdrawalRequested(
        bytes32 indexed id,
        address requester,
        uint256 amount,
        bytes20 torusAddress,
        uint256 deadline
    );
}

#[derive(Debug, Clone)]
pub struct WithdrawalEvent {
    pub id: B256,
    pub requester: Address,
    pub amount: u64,
    pub torus_address: [u8; 20],
    pub deadline: u64,
    pub block_number: u64,
}

pub struct WithdrawalWatcher {
    rpc_url: String,
    controller_addr: Address,
}

impl WithdrawalWatcher {
    pub fn new(rpc_url: &str, controller_addr: Address) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            controller_addr,
        }
    }

    pub async fn watch(&self, from_block: u64) -> Result<(Vec<WithdrawalEvent>, u64)> {
        let provider = ProviderBuilder::new().connect_http(self.rpc_url.parse()?);

        let to_block = provider.get_block_number().await?;

        if from_block > to_block {
            return Ok((Vec::new(), from_block));
        }

        let filter = Filter::new()
            .address(self.controller_addr)
            .event_signature(WithdrawalRequested::SIGNATURE_HASH)
            .from_block(from_block)
            .to_block(to_block);

        let logs = provider.get_logs(&filter).await?;

        let mut events = Vec::new();

        for log in &logs {
            let topics = log.inner.data.topics();
            let data = log.inner.data.data.as_ref();

            if topics.len() < 2 {
                continue;
            }

            let id = topics[1];

            if data.len() < 128 {
                continue;
            }

            let requester = Address::from_slice(&data[12..32]);
            let amount = U256::from_be_slice(&data[32..64]).to::<u64>();

            let mut torus_address = [0u8; 20];
            torus_address.copy_from_slice(&data[64..84]);

            let deadline = U256::from_be_slice(&data[96..128]).to::<u64>();

            let block_number = log.block_number.unwrap_or(0);

            events.push(WithdrawalEvent {
                id,
                requester,
                amount,
                torus_address,
                deadline,
                block_number,
            });
        }

        info!(
            "Found {} withdrawal events (blocks {}..{})",
            events.len(),
            from_block,
            to_block,
        );

        Ok((events, to_block))
    }
}
