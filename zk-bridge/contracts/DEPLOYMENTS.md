# Sepolia Deployments (chainId 11155111)

Deployer: `0xBd821A46F987B20989aEaAdB906505585adC206c`

| Contract | Address | Notes |
|---|---|---|
| ThresholdVerifier | `0x066d342098d2637421f8a305082ae9ec72dfd85e` | 1-of-1 multisig (deployer) |
| WrappedTRS (wTRS) | `0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a` | ERC20, minDeposit 100 TRS, maxDeposit 1M TRS |
| SP1Verifier | `0xcc3D3315dD66B38Ca87FD31822d1B6706dfFadEF` | Groth16 via SP1 Gateway `0x397A5f7f` |
| BridgeController | `0x43173d303715F4960aa09EDc41Ac811BBDdA3B17` | minStake 0.01 ETH, timeout 1hr, cooldown 1hr |

Program vKey: `0x0083951d6e5d5cebf863e820c2317a8648861df8dc3f373a926a9e52df274dda`

## Deployment History

- **Session 7**: ThresholdVerifier, WrappedTRS, SP1Verifier (commit 5f33938)
- **Session 14**: BridgeController (deploy script `DeployBridgeController.s.sol`)

## Relayer Env Vars

```
SEPOLIA_RPC_URL=https://ethereum-sepolia-rpc.publicnode.com
WTRS_CONTRACT=0x4e01C78c4EE00B77df2f23bfEA70a1194A2E963a
BRIDGE_CONTROLLER_ADDRESS=0x43173d303715F4960aa09EDc41Ac811BBDdA3B17
SEPOLIA_PRIVATE_KEY=<from .env BSC_TEST_KEY>
```
