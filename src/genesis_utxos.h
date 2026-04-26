#ifndef GENESIS_UTXOS_H
#define GENESIS_UTXOS_H

#include <vector>

struct GenesisUTXO {
    const char* scriptPubKeyHex;  // hex-encoded scriptPubKey
    int64_t amount;               // satoshis
};

// Placeholder data - replace with actual dumputxoset output before launch
// Each entry is a P2PKH script: OP_DUP OP_HASH160 <20-byte-hash> OP_EQUALVERIFY OP_CHECKSIG
static const std::vector<GenesisUTXO> GENESIS_UTXOS = {
    {"76a914a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b088ac", 50000000000LL},
    {"76a914b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0a188ac", 25000000000LL},
    {"76a914c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0a1b288ac", 10000000000LL},
    {"76a914d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0a1b2c388ac",  5000000000LL},
    {"76a914e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0a1b2c3d488ac",  1000000000LL},
};

#endif // GENESIS_UTXOS_H
