use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessedDeposit {
    pub block_hash: String,
    pub amount: u64,
    pub recipient: String,
    pub mint_tx: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RelayerState {
    pub last_block_hash: Option<String>,
    pub processed_deposits: HashMap<String, ProcessedDeposit>,

    #[serde(default)]
    pub last_withdrawal_block: u64,
    #[serde(default)]
    pub processed_withdrawals: HashSet<String>,

    #[serde(skip)]
    file_path: PathBuf,
}

impl RelayerState {
    pub fn load(path: &str) -> Result<Self> {
        let file_path = PathBuf::from(path);
        if file_path.exists() {
            let data = std::fs::read_to_string(&file_path)?;
            let mut state: Self = serde_json::from_str(&data)?;
            state.file_path = file_path;
            Ok(state)
        } else {
            Ok(Self {
                file_path,
                ..Default::default()
            })
        }
    }

    pub fn save(&self) -> Result<()> {
        let data = serde_json::to_string_pretty(self)?;
        let tmp = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &self.file_path)?;
        Ok(())
    }

    pub fn is_processed(&self, txid: &str) -> bool {
        self.processed_deposits.contains_key(txid)
    }

    pub fn mark_processed(
        &mut self,
        txid: &str,
        block_hash: &str,
        amount: u64,
        recipient: &str,
        mint_tx: &str,
    ) -> Result<()> {
        self.processed_deposits.insert(
            txid.to_string(),
            ProcessedDeposit {
                block_hash: block_hash.to_string(),
                amount,
                recipient: recipient.to_string(),
                mint_tx: mint_tx.to_string(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_secs(),
            },
        );
        self.save()?;
        Ok(())
    }

    pub fn mark_withdrawal_processed(&mut self, id: &str) -> Result<()> {
        self.processed_withdrawals.insert(id.to_string());
        self.save()
    }

    pub fn is_withdrawal_processed(&self, id: &str) -> bool {
        self.processed_withdrawals.contains(id)
    }
}
