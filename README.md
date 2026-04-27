# TORUS

Torus is a new chain upgraded from [torus-core](https://github.com/torus-economy/torus-core). The genesis block carries a balance snapshot taken at block **835,771** of the original chain, preserving all existing balances. From there, the chain starts fresh at block 0 with new features including permanent staking, covenant opcodes, and a ZK bridge.

**Existing users** can migrate by importing their `wallet.dat` file or private keys from the original torus-core wallet — all balances from the snapshot are immediately spendable on the new chain.

Ticker: TRS \
Proof of Stake: 5% yearly rate \
Min. stake age: 8 hours \
Block time: 120 sec

- Burn address: [TEuWjbJPZiuzbhuS6YFE5v4gGzkkt26HDJ](https://explorer.torus.cc/address/TEuWjbJPZiuzbhuS6YFE5v4gGzkkt26HDJ) \
  [See](contrib/burn-address.py) for more details.
- UBI Pool address:

---

## Run a node

Make sure to have a valid `TORUS.conf` file in `/home/$USER/.TORUS/TORUS.conf` or in any other path that was specified.
For more information about configuration file see [example](TORUS.conf).

Minimum `TORUS.conf` configuration file should include the following:

```bash
rpcuser=rpc
rpcpassword=password123
server=1
listen=1
```

#### Seed nodes

Official seed nodes:

- 95.111.231.121

Official DNS seed servers:

-

### Build from source

In order to build from source, check out [docs](doc). Specific dependencies can be found [here](doc/dependencies.md).

## Release notes

To see release notes check out this [file](doc/release-notes.md).

## ZK Bridge (Experimental)

> **This feature is in testing and not yet production-ready. Contracts are deployed on Sepolia testnet only.**

The ZK bridge enables trustless cross-chain transfers between Torus (TRS) and EVM chains using zero-knowledge proofs. Deposits on the Torus chain are verified on-chain using SP1 Groth16 proofs, allowing minting of wrapped TRS (wTRS) without trusted intermediaries.

### Architecture

| Component | Language | Description |
|-----------|----------|-------------|
| **Contracts** | Solidity | ERC-20 wTRS token, SP1 Groth16 verifier, operator-staked BridgeController |
| **Relayer** | Rust | Watches both chains, generates ZK proofs, submits mint/withdrawal transactions |
| **ZK Circuit** | Rust (SP1 zkVM) | Proves PoS block validity, transaction inclusion via Merkle proof |

### Contracts (Sepolia)

| Contract | Address | Role |
|----------|---------|------|
| WrappedTRS (wTRS) | `0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a` | ERC-20 token (8 decimals), handles mint/withdraw with swappable verifier |
| SP1Verifier | `0xcc3D3315dD66B38Ca87FD31822d1B6706dfFadEF` | Validates Groth16 proofs via SP1 gateway |
| ThresholdVerifier | `0x066d342098d2637421f8a305082ae9ec72dfd85e` | EIP-712 t-of-n multisig (Phase A fallback) |
| BridgeController | `0x43173d303715F4960aa09EDc41Ac811BBDdA3B17` | Operator registry, withdrawal lifecycle, slashing |

### ZK Circuit

The SP1 guest program proves three things for each bridge transaction:

1. Block header hashes correctly to the claimed `blockHash`
2. PoS stake kernel hash meets the difficulty target
3. Transaction hash appears in the block's Merkle tree

Public outputs are 192 bytes (6 x 32-byte words): `mode | blockHash | kernelHash | txHash | amount | recipient`.

### Deposit Flow (TRS → wTRS)

1. User sends TRS to the bridge address on Torus
2. Relayer detects the deposit after 6 confirmations
3. Relayer generates a Groth16 proof via the SP1 Prover Network (~35s)
4. Relayer calls `WrappedTRS.mint()` on EVM — verifier validates the proof, wTRS is minted 1:1

### Withdrawal Flow (wTRS → TRS)

1. User calls `BridgeController.requestWithdrawal(amount, torusAddress)` — wTRS held by controller, 1-hour deadline starts
2. User posts a Torus-side ECDSA authorization to the relayer API
3. Relayer builds and broadcasts a raw Torus transaction (P2PKH + OP_RETURN with EVM address)
4. After 6 confirmations, relayer generates a withdrawal ZK proof and calls `BridgeController.confirmWithdrawal()` — wTRS is burned
5. If not confirmed within 1 hour, anyone can call `slashForTimeout()` — wTRS returned to user, 50% of each operator's ETH stake is slashed

### Current Limitations

- Deployed on Sepolia testnet only — no mainnet deployment
- Operator set is 1-of-1 (single operator) — decentralization not yet live
- WrappedTRS still uses ThresholdVerifier — SP1Verifier upgrade pending
- Withdrawal path requires manual user signature submission to relayer API

See [zk-bridge/](zk-bridge/) for contracts, relayer, and ZK circuits.
