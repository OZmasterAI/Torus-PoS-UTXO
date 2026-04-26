// Copyright (c) 2010 Satoshi Nakamoto
// Copyright (c) 2009-2012 The Bitcoin developers
// Distributed under the MIT/X11 software license, see the accompanying
// file COPYING or http://www.opensource.org/licenses/mit-license.php.

#include "main.h"
#include "bitcoinrpc.h"
#include "base58.h"
#include <fstream>
#include <boost/filesystem.hpp>

using namespace json_spirit;
using namespace std;

extern void TxToJSON(const CTransaction& tx, const uint256 hashBlock, json_spirit::Object& entry);
extern enum Checkpoints::CPMode CheckpointsMode;

double GetDifficulty(const CBlockIndex* blockindex)
{
    // Floating point number that is a multiple of the minimum difficulty,
    // minimum difficulty = 1.0.
    if (blockindex == NULL)
    {
        if (pindexBest == NULL)
            return 1.0;
        else
            blockindex = GetLastBlockIndex(pindexBest, false);
    }

    int nShift = (blockindex->nBits >> 24) & 0xff;

    double dDiff =
        (double)0x0000ffff / (double)(blockindex->nBits & 0x00ffffff);

    while (nShift < 29)
    {
        dDiff *= 256.0;
        nShift++;
    }
    while (nShift > 29)
    {
        dDiff /= 256.0;
        nShift--;
    }

    return dDiff;
}

double GetPoWMHashPS()
{
    if (pindexBest->nHeight >= LAST_POW_BLOCK)
        return 0;

    int nPoWInterval = 72;
    int64_t nTargetSpacingWorkMin = 30, nTargetSpacingWork = 30;

    CBlockIndex* pindex = pindexGenesisBlock;
    CBlockIndex* pindexPrevWork = pindexGenesisBlock;

    while (pindex)
    {
        if (pindex->IsProofOfWork())
        {
            int64_t nActualSpacingWork = pindex->GetBlockTime() - pindexPrevWork->GetBlockTime();
            nTargetSpacingWork = ((nPoWInterval - 1) * nTargetSpacingWork + nActualSpacingWork + nActualSpacingWork) / (nPoWInterval + 1);
            nTargetSpacingWork = max(nTargetSpacingWork, nTargetSpacingWorkMin);
            pindexPrevWork = pindex;
        }

        pindex = pindex->pnext;
    }

    return GetDifficulty() * 4294.967296 / nTargetSpacingWork;
}

double GetPoSKernelPS()
{
    int nPoSInterval = 72;
    double dStakeKernelsTriedAvg = 0;
    int nStakesHandled = 0, nStakesTime = 0;

    CBlockIndex* pindex = pindexBest;;
    CBlockIndex* pindexPrevStake = NULL;

    while (pindex && nStakesHandled < nPoSInterval)
    {
        if (pindex->IsProofOfStake())
        {
            dStakeKernelsTriedAvg += GetDifficulty(pindex) * 4294967296.0;
            nStakesTime += pindexPrevStake ? (pindexPrevStake->nTime - pindex->nTime) : 0;
            pindexPrevStake = pindex;
            nStakesHandled++;
        }

        pindex = pindex->pprev;
    }

    return nStakesTime ? dStakeKernelsTriedAvg / nStakesTime : 0;
}

Object blockToJSON(const CBlock& block, const CBlockIndex* blockindex, bool fPrintTransactionDetail)
{
    Object result;
    result.push_back(Pair("hash", block.GetHash().GetHex()));
    CMerkleTx txGen(block.vtx[0]);
    txGen.SetMerkleBranch(&block);
    result.push_back(Pair("confirmations", (int)txGen.GetDepthInMainChain()));
    result.push_back(Pair("size", (int)::GetSerializeSize(block, SER_NETWORK, PROTOCOL_VERSION)));
    result.push_back(Pair("height", blockindex->nHeight));
    result.push_back(Pair("version", block.nVersion));
    result.push_back(Pair("merkleroot", block.hashMerkleRoot.GetHex()));
    result.push_back(Pair("mint", ValueFromAmount(blockindex->nMint)));
    result.push_back(Pair("time", (int64_t)block.GetBlockTime()));
    result.push_back(Pair("nonce", (uint64_t)block.nNonce));
    result.push_back(Pair("bits", HexBits(block.nBits)));
    result.push_back(Pair("difficulty", GetDifficulty(blockindex)));
    result.push_back(Pair("blocktrust", leftTrim(blockindex->GetBlockTrust().GetHex(), '0')));
    result.push_back(Pair("chaintrust", leftTrim(blockindex->nChainTrust.GetHex(), '0')));
    if (blockindex->pprev)
        result.push_back(Pair("previousblockhash", blockindex->pprev->GetBlockHash().GetHex()));
    if (blockindex->pnext)
        result.push_back(Pair("nextblockhash", blockindex->pnext->GetBlockHash().GetHex()));

    result.push_back(Pair("flags", strprintf("%s%s", blockindex->IsProofOfStake()? "proof-of-stake" : "proof-of-work", blockindex->GeneratedStakeModifier()? " stake-modifier": "")));
    result.push_back(Pair("proofhash", blockindex->hashProof.GetHex()));
    result.push_back(Pair("entropybit", (int)blockindex->GetStakeEntropyBit()));
    result.push_back(Pair("modifier", strprintf("%016" PRIx64, blockindex->nStakeModifier)));
    result.push_back(Pair("modifierchecksum", strprintf("%08x", blockindex->nStakeModifierChecksum)));
    Array txinfo;
    for (const CTransaction& tx : block.vtx)
    {
        if (fPrintTransactionDetail)
        {
            Object entry;

            entry.push_back(Pair("txid", tx.GetHash().GetHex()));
            TxToJSON(tx, 0, entry);

            txinfo.push_back(entry);
        }
        else
            txinfo.push_back(tx.GetHash().GetHex());
    }

    result.push_back(Pair("tx", txinfo));

    if (block.IsProofOfStake())
        result.push_back(Pair("signature", HexStr(block.vchBlockSig.begin(), block.vchBlockSig.end())));

    return result;
}

Value getbestblockhash(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 0)
        throw runtime_error(
            "getbestblockhash\n"
            "Returns the hash of the best block in the longest block chain.");

    return hashBestChain.GetHex();
}

Value getblockcount(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 0)
        throw runtime_error(
            "getblockcount\n"
            "Returns the number of blocks in the longest block chain.");

    return nBestHeight;
}


Value getdifficulty(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 0)
        throw runtime_error(
            "getdifficulty\n"
            "Returns the difficulty as a multiple of the minimum difficulty.");

    Object obj;
    obj.push_back(Pair("proof-of-work",        GetDifficulty()));
    obj.push_back(Pair("proof-of-stake",       GetDifficulty(GetLastBlockIndex(pindexBest, true))));
    obj.push_back(Pair("search-interval",      (int)nLastCoinStakeSearchInterval));
    return obj;
}


Value settxfee(const Array& params, bool fHelp)
{
    if (fHelp || params.size() < 1 || params.size() > 1 || AmountFromValue(params[0]) < MIN_TX_FEE)
        throw runtime_error(
            "settxfee <amount>\n"
            "<amount> is a real and is rounded to the nearest 0.01");

    nTransactionFee = AmountFromValue(params[0]);
    nTransactionFee = (nTransactionFee / CENT) * CENT;  // round to cent

    return true;
}

Value getrawmempool(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 0)
        throw runtime_error(
            "getrawmempool\n"
            "Returns all transaction ids in memory pool.");

    vector<uint256> vtxid;
    mempool.queryHashes(vtxid);

    Array a;
    for (const uint256& hash : vtxid)
        a.push_back(hash.ToString());

    return a;
}

Value getblockhash(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 1)
        throw runtime_error(
            "getblockhash <index>\n"
            "Returns hash of block in best-block-chain at <index>.");

    int nHeight = params[0].get_int();
    if (nHeight < 0 || nHeight > nBestHeight)
        throw runtime_error("Block number out of range.");

    CBlockIndex* pblockindex = FindBlockByHeight(nHeight);
    return pblockindex->phashBlock->GetHex();
}

Value getblock(const Array& params, bool fHelp)
{
    if (fHelp || params.size() < 1 || params.size() > 2)
        throw runtime_error(
            "getblock <hash> [txinfo]\n"
            "txinfo optional to print more detailed tx info\n"
            "Returns details of a block with given block-hash.");

    std::string strHash = params[0].get_str();
    uint256 hash(strHash);

    if (mapBlockIndex.count(hash) == 0)
        throw JSONRPCError(RPC_INVALID_ADDRESS_OR_KEY, "Block not found");

    CBlock block;
    CBlockIndex* pblockindex = mapBlockIndex[hash];
    block.ReadFromDisk(pblockindex, true);

    return blockToJSON(block, pblockindex, params.size() > 1 ? params[1].get_bool() : false);
}

Value getblockbynumber(const Array& params, bool fHelp)
{
    if (fHelp || params.size() < 1 || params.size() > 2)
        throw runtime_error(
            "getblockbynumber <number> [txinfo]\n"
            "txinfo optional to print more detailed tx info\n"
            "Returns details of a block with given block-number.");

    int nHeight = params[0].get_int();
    if (nHeight < 0 || nHeight > nBestHeight)
        throw runtime_error("Block number out of range.");

    CBlock block;
    CBlockIndex* pblockindex = mapBlockIndex[hashBestChain];
    while (pblockindex->nHeight > nHeight)
        pblockindex = pblockindex->pprev;

    uint256 hash = *pblockindex->phashBlock;

    pblockindex = mapBlockIndex[hash];
    block.ReadFromDisk(pblockindex, true);

    return blockToJSON(block, pblockindex, params.size() > 1 ? params[1].get_bool() : false);
}

// ppcoin: get information of sync-checkpoint
Value getcheckpoint(const Array& params, bool fHelp)
{
    if (fHelp || params.size() != 0)
        throw runtime_error(
            "getcheckpoint\n"
            "Show info of synchronized checkpoint.\n");

    Object result;
    CBlockIndex* pindexCheckpoint;

    result.push_back(Pair("synccheckpoint", Checkpoints::hashSyncCheckpoint.ToString().c_str()));
    pindexCheckpoint = mapBlockIndex[Checkpoints::hashSyncCheckpoint];
    result.push_back(Pair("height", pindexCheckpoint->nHeight));
    result.push_back(Pair("timestamp", DateTimeStrFormat(pindexCheckpoint->GetBlockTime()).c_str()));

    // Check that the block satisfies synchronized checkpoint
    if (CheckpointsMode == Checkpoints::STRICT)
        result.push_back(Pair("policy", "strict"));

    if (CheckpointsMode == Checkpoints::ADVISORY)
        result.push_back(Pair("policy", "advisory"));

    if (CheckpointsMode == Checkpoints::PERMISSIVE)
        result.push_back(Pair("policy", "permissive"));

    if (mapArgs.count("-checkpointkey"))
        result.push_back(Pair("checkpointmaster", true));

    return result;
}

Value dumputxoset(const Array& params, bool fHelp)
{
    if (fHelp || params.size() > 1)
        throw runtime_error(
            "dumputxoset [height]\n"
            "Dumps all unspent transaction outputs (UTXOs) at the given block height\n"
            "to a JSON file in the data directory.\n"
            "If height is not specified, uses the current best block height.\n"
            "Returns a summary with height, UTXO count, and total amount.");

    // Determine target height
    int nTargetHeight = nBestHeight;
    if (params.size() > 0)
    {
        nTargetHeight = params[0].get_int();
        if (nTargetHeight < 0 || nTargetHeight > nBestHeight)
            throw JSONRPCError(RPC_INVALID_PARAMETER, "Block height out of range");
    }

    // Find the block index at the target height
    CBlockIndex* pindexTarget = pindexGenesisBlock;
    while (pindexTarget && pindexTarget->nHeight < nTargetHeight)
        pindexTarget = pindexTarget->pnext;

    if (!pindexTarget || pindexTarget->nHeight != nTargetHeight)
        throw JSONRPCError(RPC_INVALID_PARAMETER, "Block not found at specified height");

    // Walk the chain from genesis to target height, collecting all transaction
    // outputs and removing spent ones to build the complete UTXO set.
    map<COutPoint, CTxOut> mapUTXOs;

    printf("dumputxoset: scanning blocks 0 to %d...\n", nTargetHeight);

    CBlockIndex* pindex = pindexGenesisBlock;
    while (pindex && pindex->nHeight <= nTargetHeight)
    {
        CBlock block;
        if (!block.ReadFromDisk(pindex, true))
            throw JSONRPCError(RPC_INTERNAL_ERROR,
                strprintf("Failed to read block at height %d", pindex->nHeight));

        for (unsigned int i = 0; i < block.vtx.size(); i++)
        {
            const CTransaction& tx = block.vtx[i];
            uint256 txhash = tx.GetHash();

            // Mark spent inputs - remove from UTXO set
            if (!tx.IsCoinBase())
            {
                for (const CTxIn& txin : tx.vin)
                {
                    mapUTXOs.erase(txin.prevout);
                }
            }

            // Add new outputs to the UTXO set
            for (unsigned int n = 0; n < tx.vout.size(); n++)
            {
                const CTxOut& txout = tx.vout[n];

                // Skip empty outputs (e.g., PoS marker output)
                if (txout.IsEmpty())
                    continue;

                // Skip zero-value outputs
                if (txout.nValue == 0)
                    continue;

                mapUTXOs[COutPoint(txhash, n)] = txout;
            }
        }

        if (pindex->nHeight % 10000 == 0)
            printf("dumputxoset: processed block %d...\n", pindex->nHeight);

        pindex = pindex->pnext;
    }

    printf("dumputxoset: found %u unspent outputs, building JSON...\n",
        (unsigned int)mapUTXOs.size());

    // Build the JSON output
    Array utxoArray;
    int64_t nTotalAmount = 0;

    for (map<COutPoint, CTxOut>::const_iterator it = mapUTXOs.begin();
         it != mapUTXOs.end(); ++it)
    {
        const COutPoint& outpoint = it->first;
        const CTxOut& txout = it->second;

        Object utxoEntry;

        // Extract address from scriptPubKey
        CTxDestination dest;
        if (ExtractDestination(txout.scriptPubKey, dest))
        {
            CBitcoinAddress addr(dest);
            utxoEntry.push_back(Pair("address", addr.ToString()));
        }
        else
        {
            utxoEntry.push_back(Pair("address", string("unknown")));
        }

        // scriptPubKey as hex
        utxoEntry.push_back(Pair("scriptPubKey",
            HexStr(txout.scriptPubKey.begin(), txout.scriptPubKey.end())));

        // Amount in satoshis
        utxoEntry.push_back(Pair("amount", txout.nValue));

        // txid and vout
        utxoEntry.push_back(Pair("txid", outpoint.hash.GetHex()));
        utxoEntry.push_back(Pair("vout", (int)outpoint.n));

        nTotalAmount += txout.nValue;
        utxoArray.push_back(utxoEntry);
    }

    // Build the top-level JSON object
    Object result;
    result.push_back(Pair("height", nTargetHeight));

    // ISO 8601 timestamp
    string strTimestamp = DateTimeStrFormat("%Y-%m-%dT%H:%M:%SZ",
        pindexTarget->GetBlockTime());
    result.push_back(Pair("timestamp", strTimestamp));
    result.push_back(Pair("utxos", utxoArray));

    // Write to file
    boost::filesystem::path pathSnapshot = GetDataDir() /
        strprintf("utxo_snapshot_%d.json", nTargetHeight);

    std::ofstream file(pathSnapshot.string().c_str());
    if (!file.is_open())
        throw JSONRPCError(RPC_INTERNAL_ERROR,
            "Failed to open output file: " + pathSnapshot.string());

    string strJSON = write_string(Value(result), true);
    file << strJSON;
    file.close();

    printf("dumputxoset: wrote %u UTXOs to %s\n",
        (unsigned int)utxoArray.size(), pathSnapshot.string().c_str());

    // Return summary
    Object summary;
    summary.push_back(Pair("height", nTargetHeight));
    summary.push_back(Pair("utxo_count", (int)utxoArray.size()));
    summary.push_back(Pair("total_amount", nTotalAmount));
    summary.push_back(Pair("file", pathSnapshot.string()));

    return summary;
}
