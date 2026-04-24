use crate::prover::ProofResult;
use alloy::network::EthereumWallet;
use alloy::primitives::{Address, Bytes, FixedBytes, U256};
use alloy::providers::ProviderBuilder;
use alloy::signers::local::PrivateKeySigner;
use eyre::Result;
use std::str::FromStr;
use tracing::info;

alloy::sol! {
    #[sol(rpc)]
    contract WrappedTRS {
        function mint(
            bytes calldata proof,
            bytes32 blockHash,
            bytes32 txHash,
            uint256 amount,
            address recipient
        ) external;
    }
}

alloy::sol! {
    #[sol(rpc)]
    contract BridgeControllerRpc {
        function confirmWithdrawal(
            bytes32 withdrawalId,
            bytes calldata proof,
            bytes32 blockHash,
            bytes32 txHash
        ) external;
    }
}

pub struct SepoliaSubmitter {
    rpc_url: String,
    wallet: EthereumWallet,
    contract_address: Address,
    bridge_controller_address: Option<Address>,
}

impl SepoliaSubmitter {
    pub fn new(rpc_url: &str, private_key: &str, contract_address: &str) -> Result<Self> {
        let key = private_key.strip_prefix("0x").unwrap_or(private_key);
        let signer: PrivateKeySigner = key.parse()?;
        let wallet = EthereumWallet::from(signer);
        let address = Address::from_str(contract_address)?;

        Ok(Self {
            rpc_url: rpc_url.to_string(),
            wallet,
            contract_address: address,
            bridge_controller_address: None,
        })
    }

    pub fn with_bridge_controller(mut self, address: &str) -> Result<Self> {
        self.bridge_controller_address = Some(Address::from_str(address)?);
        Ok(self)
    }

    pub async fn submit_mint(&self, proof_result: &ProofResult) -> Result<String> {
        let provider = ProviderBuilder::new()
            .wallet(self.wallet.clone())
            .connect_http(self.rpc_url.parse()?);

        let contract = WrappedTRS::new(self.contract_address, &provider);

        let block_hash = FixedBytes::from(proof_result.block_hash_be);
        let tx_hash = FixedBytes::from(proof_result.tx_hash_be);
        let amount = U256::from(proof_result.amount);
        let recipient = Address::from(proof_result.recipient);
        let proof = Bytes::from(proof_result.calldata.clone());

        info!("Submitting mint to Sepolia...");
        info!("  Block hash: {}", block_hash);
        info!("  TX hash:    {}", tx_hash);
        info!("  Amount:     {} TRS", proof_result.amount as f64 / 1e8);
        info!("  Recipient:  {}", recipient);

        info!("Tx sent, waiting for confirmation...");
        let receipt = contract
            .mint(proof, block_hash, tx_hash, amount, recipient)
            .send()
            .await?
            .get_receipt()
            .await?;

        let mint_tx_hash = format!("{}", receipt.transaction_hash);
        info!("Mint confirmed: {}", mint_tx_hash);

        Ok(mint_tx_hash)
    }

    pub async fn confirm_withdrawal(
        &self,
        withdrawal_id: FixedBytes<32>,
        proof: Vec<u8>,
        block_hash: FixedBytes<32>,
        tx_hash: FixedBytes<32>,
    ) -> Result<String> {
        let controller_addr = self
            .bridge_controller_address
            .ok_or_else(|| eyre::eyre!("bridge controller address not configured"))?;

        let provider = ProviderBuilder::new()
            .wallet(self.wallet.clone())
            .connect_http(self.rpc_url.parse()?);

        let contract = BridgeControllerRpc::new(controller_addr, &provider);

        info!("Submitting confirmWithdrawal to BridgeController...");
        info!("  Withdrawal ID: {}", withdrawal_id);
        info!("  Block hash:    {}", block_hash);
        info!("  TX hash:       {}", tx_hash);

        let receipt = contract
            .confirmWithdrawal(withdrawal_id, Bytes::from(proof), block_hash, tx_hash)
            .send()
            .await?
            .get_receipt()
            .await?;

        let confirm_tx_hash = format!("{}", receipt.transaction_hash);
        info!("Withdrawal confirmed: {}", confirm_tx_hash);

        Ok(confirm_tx_hash)
    }
}
