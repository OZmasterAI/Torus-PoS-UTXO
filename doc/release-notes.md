# TORUS Release Notes

*****************************

## Release v2.0.0 - Permanent Staking (Hard Fork)

**IMPORTANT: This release contains consensus changes. All nodes must upgrade before block 450,000.**

#### Added

- **Permanent Locked Staking** — Irreversibly lock TRS for staking rewards
    - New `OP_PERMANENT_LOCK` opcode and `TX_PERMANENT_STAKE` script type
    - Locked coins stake automatically and earn rewards paid as liquid (spendable) UTXOs
    - The locked principal can never be spent or unlocked
    - Minimum lock amount: 100 TRS
    - Locked stakes receive 4x the normal coin age cap (360 days vs 90 days), giving them consistently higher staking priority
    - Consensus enforced: non-coinstake transactions cannot spend locked outputs, coinstake must re-lock the full principal
- New RPC command: `permanentlock <amount>` — creates a permanent stake transaction
- `getstakinginfo` now reports `permanentstakebalance` and `permanentstakecount`
- `getinfo` now shows `permanentstake` balance separately from spendable balance
- Wallet GUI: permanent stake balance displayed on overview page
- Transaction history: permanent stake lock transactions shown with distinct label

#### Changed

- `GetBalance()` excludes permanently locked coins from spendable balance
- `AvailableCoins()` filters out permanent stake UTXOs from regular coin selection
- `GetWeight()` accepts optional permanent stake flag for extended age cap

#### Fixed

- OpenSSL 3.0 compatibility: `BN_zero()` void return handling in `key.cpp`
- glibc 2.38+ compatibility: `strlcpy`/`strlcat` redefinition guards in `strlcpy.h`

#### Activation

- Block height: **450,000**
- Before activation: permanent stake outputs are not recognized by consensus
- After activation: all consensus rules are enforced

*****************************

## Release [v1.0.3](https://github.com/torus-economy/torus-core/releases/tag/v1.0.3) - 05 Aug 2023

#### Added

- Updated docs and seed nodes
    - [`#fb48326`](https://github.com/torus-economy/torus-core/commit/fb4832653cabe38bc8d436589ad5547bca3ae1ea)
    - [`#f1eed2d`](https://github.com/torus-economy/torus-core/commit/f1eed2d6dc2d5ac8f6b8c9a97f59e3794621fa0b)
- Added Burn address and UBI Pool address
    - [`#64a524d`](https://github.com/torus-economy/torus-core/commit/64a524dce71cb22056e3160452fb5bf9364992c5)
- Added new block checkpoints
    - [`#228b4a7`](https://github.com/torus-economy/torus-core/commit/228b4a7c87225f0387538854769d69c1e5254f64)
- Added TORUS-Qt wallet for Unix
    - [`#d7fe1c4`](https://github.com/torus-economy/torus-core/commit/d7fe1c41bf346cf58031ef190ee8ceff83380a87)

#### Changed

- Improved initial block synchronisation
    - [`#4b0b069`](https://github.com/torus-economy/torus-core/commit/4b0b069d3371ea9e8e218c61066b250a61c49a03)

#### Fixed

- Fixed Docker builds
    - [`#f2bb859`](https://github.com/torus-economy/torus-core/commit/f2bb8590a1b2756cb8b062c457238b3492f29e24)

*****************************

## Release [v1.0.2](https://github.com/torus-economy/torus-core/releases/tag/v1.0.2) - 26 Mar 2022

#### Added

- Added QR code support
    - [`#73065ec`](https://github.com/torus-economy/torus-core/commit/73065ec2b3990509e488c5dbd8fe4d78bc970b5b)
- Logged LevelDB version
    - [`#f3f2ff9`](https://github.com/torus-economy/torus-core/commit/f3f2ff9da192106622306790b987bafb25d9457e)
- Zipped release artifacts
    - [`#8e3fba4`](https://github.com/torus-economy/torus-core/commit/8e3fba42ac411e13b818e52d077b89cec32b5357)

#### Changed

- Minimized wallet on app close
    - [`#dbf4f17`](https://github.com/torus-economy/torus-core/commit/dbf4f17f6b08be439e8a8089fa00ee5c69992ad2)
- Improved CI builds with prebuilt images
    - [`#1a7f2db`](https://github.com/torus-economy/torus-core/commit/1a7f2db29ada228420b57e58417406004fc2f23d)
    - [`#6cdaff7`](https://github.com/torus-economy/torus-core/commit/6cdaff7eaf726a4aedf0201b4c95e7347a246f42)
- Updated documentation

#### Removed

- Removed custom wallet theme
    - [`#3ac29c1`](https://github.com/torus-economy/torus-core/commit/3ac29c1b360e73af7e63e2561aece8c91cc89b57)

#### Fixed

- Fixed wallet style and tray icon
    - [`#9378a68`](https://github.com/torus-economy/torus-core/commit/9378a6850b17fbc0151bf70ae7ac4c45fb734432)

*****************************

## Release [v1.0.1](https://github.com/torus-economy/torus-core/releases/tag/v1.0.1) - 20 Feb 2022

#### Added

- Added official DNS seed servers
    - [`#afedd4c`](https://github.com/torus-economy/torus-core/commit/afedd4cfa5f253478b364f4ef7fe27ba8dcc5bc5)
- Supported multiplatform builds and CI pipeline
    - [`#4231267`](https://github.com/torus-economy/torus-core/commit/42312672c6257dbb0e801e4bfc94c4304ea0296b)
- Improved documentation

#### Changed

- Enabled UPnP in TORUS-Qt
    - [`#300c2b2`](https://github.com/torus-economy/torus-core/commit/300c2b28cf00b3d5ba5e48db3de7feb1484a2234)

#### Fixed

- Removed duplicate key from RPC wallet
    - [`#19ce246`](https://github.com/torus-economy/torus-core/commit/19ce246f2ff9a1a54ba81edd7d4153840b27a6e7)

*****************************

## Release [v1.0.0](https://github.com/torus-economy/torus-core/releases/tag/v1.0.0) - 06 Dec 2021

**MAINNET LAUNCHED**
