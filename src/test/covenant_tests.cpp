#include <boost/test/unit_test.hpp>

#include "main.h"
#include "wallet.h"
#include "script.h"
#include "key.h"
#include "uint256.h"

using namespace std;

extern uint256 SignatureHash(CScript scriptCode, const CTransaction& txTo, unsigned int nIn, int nHashType);

// Helper: create a basic spending transaction with one input and one output
static CTransaction MakeSpendTx(const CTransaction& txFrom, int64_t nValue, const CScript& scriptPubKeyOut)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vin[0].prevout.hash = txFrom.GetHash();
    tx.vin[0].prevout.n = 0;
    tx.vin[0].nSequence = 0;
    tx.vout.resize(1);
    tx.vout[0].nValue = nValue;
    tx.vout[0].scriptPubKey = scriptPubKeyOut;
    return tx;
}

// ==========================================
// OP_CHECKLOCKTIMEVERIFY tests
// ==========================================
BOOST_AUTO_TEST_SUITE(cltv_tests)

BOOST_AUTO_TEST_CASE(cltv_basic_pass)
{
    CScript scriptPubKey;
    scriptPubKey << CBigNum(1000) << OP_CHECKLOCKTIMEVERIFY << OP_DROP << OP_TRUE;

    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].scriptPubKey = scriptPubKey;

    CTransaction txTo = MakeSpendTx(txFrom, 1, CScript());
    txTo.nLockTime = 1000;

    CScript scriptSig;
    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptSig, txTo, 0, 0, SCRIPT_VERIFY_COVENANTS));
    BOOST_CHECK(EvalScript(stack, scriptPubKey, txTo, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(cltv_locktime_too_low)
{
    CScript scriptPubKey;
    scriptPubKey << CBigNum(2000) << OP_CHECKLOCKTIMEVERIFY << OP_DROP << OP_TRUE;

    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].scriptPubKey = scriptPubKey;

    CTransaction txTo = MakeSpendTx(txFrom, 1, CScript());
    txTo.nLockTime = 999;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, txTo, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(cltv_sequence_final_fails)
{
    CScript scriptPubKey;
    scriptPubKey << CBigNum(1000) << OP_CHECKLOCKTIMEVERIFY << OP_DROP << OP_TRUE;

    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].scriptPubKey = scriptPubKey;

    CTransaction txTo = MakeSpendTx(txFrom, 1, CScript());
    txTo.nLockTime = 1000;
    txTo.vin[0].nSequence = std::numeric_limits<unsigned int>::max();

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, txTo, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(cltv_type_mismatch_fails)
{
    // Block height locktime in script, unix timestamp in tx
    CScript scriptPubKey;
    scriptPubKey << CBigNum(1000) << OP_CHECKLOCKTIMEVERIFY << OP_DROP << OP_TRUE;

    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].scriptPubKey = scriptPubKey;

    CTransaction txTo = MakeSpendTx(txFrom, 1, CScript());
    txTo.nLockTime = 500000001; // Unix timestamp

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, txTo, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(cltv_nop_before_activation)
{
    CScript scriptPubKey;
    scriptPubKey << CBigNum(999999) << OP_CHECKLOCKTIMEVERIFY << OP_DROP << OP_TRUE;

    CTransaction txTo;
    txTo.vin.resize(1);
    txTo.vout.resize(1);
    txTo.nLockTime = 0;

    // Without SCRIPT_VERIFY_COVENANTS, CLTV acts as NOP (doesn't fail)
    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, txTo, 0, 0, SCRIPT_VERIFY_NONE));
}

BOOST_AUTO_TEST_SUITE_END()

// ==========================================
// OP_CHECKSIGFROMSTACKVERIFY tests
// ==========================================
BOOST_AUTO_TEST_SUITE(checksigfromstack_tests)

BOOST_AUTO_TEST_CASE(csfs_valid_signature)
{
    CKey key;
    key.MakeNewKey(true);

    valtype vchMsg(32, 0x42);
    uint256 msgHash = Hash(vchMsg.begin(), vchMsg.end());

    vector<unsigned char> vchSig;
    BOOST_CHECK(key.Sign(msgHash, vchSig));

    CScript scriptPubKey;
    scriptPubKey << OP_CHECKSIGFROMSTACKVERIFY << OP_TRUE;

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    vector<vector<unsigned char> > stack;
    stack.push_back(vchSig);
    stack.push_back(vchMsg);
    stack.push_back(key.GetPubKey().Raw());

    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(csfs_wrong_message)
{
    CKey key;
    key.MakeNewKey(true);

    valtype vchMsg(32, 0x42);
    uint256 msgHash = Hash(vchMsg.begin(), vchMsg.end());

    vector<unsigned char> vchSig;
    BOOST_CHECK(key.Sign(msgHash, vchSig));

    valtype vchWrongMsg(32, 0x99);

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    CScript scriptPubKey;
    scriptPubKey << OP_CHECKSIGFROMSTACKVERIFY << OP_TRUE;

    vector<vector<unsigned char> > stack;
    stack.push_back(vchSig);
    stack.push_back(vchWrongMsg);
    stack.push_back(key.GetPubKey().Raw());

    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(csfs_wrong_key)
{
    CKey key, wrongKey;
    key.MakeNewKey(true);
    wrongKey.MakeNewKey(true);

    valtype vchMsg(32, 0x42);
    uint256 msgHash = Hash(vchMsg.begin(), vchMsg.end());

    vector<unsigned char> vchSig;
    BOOST_CHECK(key.Sign(msgHash, vchSig));

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    CScript scriptPubKey;
    scriptPubKey << OP_CHECKSIGFROMSTACKVERIFY << OP_TRUE;

    vector<vector<unsigned char> > stack;
    stack.push_back(vchSig);
    stack.push_back(vchMsg);
    stack.push_back(wrongKey.GetPubKey().Raw());

    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(csfs_nop_before_activation)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    CScript scriptPubKey;
    scriptPubKey << OP_CHECKSIGFROMSTACKVERIFY << OP_TRUE;

    // Without flags, CSFS acts as NOP — but stack is empty, so it would fail
    // if enforced. With NOP behavior it should pass.
    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_NONE));
}

BOOST_AUTO_TEST_SUITE_END()

// ==========================================
// OP_OUTPUTAMOUNT and OP_OUTPUTSCRIPT tests
// ==========================================
BOOST_AUTO_TEST_SUITE(output_introspection_tests)

BOOST_AUTO_TEST_CASE(outputamount_correct_value)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(2);
    tx.vout[0].nValue = 50000;
    tx.vout[0].scriptPubKey = CScript() << OP_TRUE;
    tx.vout[1].nValue = 30000;
    tx.vout[1].scriptPubKey = CScript() << OP_TRUE;

    // Script: 0 OP_OUTPUTAMOUNT <50000> OP_EQUALVERIFY OP_TRUE
    CScript scriptPubKey;
    scriptPubKey << OP_0 << OP_OUTPUTAMOUNT << CBigNum(50000) << OP_EQUALVERIFY << OP_TRUE;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(outputamount_wrong_value)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);
    tx.vout[0].nValue = 50000;
    tx.vout[0].scriptPubKey = CScript() << OP_TRUE;

    CScript scriptPubKey;
    scriptPubKey << OP_0 << OP_OUTPUTAMOUNT << CBigNum(99999) << OP_EQUALVERIFY << OP_TRUE;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(outputamount_second_output)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(2);
    tx.vout[0].nValue = 50000;
    tx.vout[0].scriptPubKey = CScript() << OP_TRUE;
    tx.vout[1].nValue = 30000;
    tx.vout[1].scriptPubKey = CScript() << OP_TRUE;

    CScript scriptPubKey;
    scriptPubKey << OP_1 << OP_OUTPUTAMOUNT << CBigNum(30000) << OP_EQUALVERIFY << OP_TRUE;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(outputamount_out_of_range)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);
    tx.vout[0].nValue = 50000;

    CScript scriptPubKey;
    scriptPubKey << OP_1 << OP_OUTPUTAMOUNT;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(outputscript_correct_script)
{
    CKey bobKey;
    bobKey.MakeNewKey(true);
    CScript bobScript;
    bobScript << OP_DUP << OP_HASH160 << bobKey.GetPubKey().GetID() << OP_EQUALVERIFY << OP_CHECKSIG;

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);
    tx.vout[0].nValue = 50000;
    tx.vout[0].scriptPubKey = bobScript;

    // Push output[0] script and compare against expected
    valtype vchExpected(bobScript.begin(), bobScript.end());

    CScript scriptPubKey;
    scriptPubKey << OP_0 << OP_OUTPUTSCRIPT;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
    BOOST_CHECK(stack.size() == 1);
    BOOST_CHECK(stack[0] == vchExpected);
}

BOOST_AUTO_TEST_CASE(outputscript_out_of_range)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    CScript scriptPubKey;
    scriptPubKey << OP_5 << OP_OUTPUTSCRIPT;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(output_introspection_nop_before_activation)
{
    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);

    // Without SCRIPT_VERIFY_COVENANTS, these act as NOPs
    CScript scriptPubKey;
    scriptPubKey << OP_0 << OP_OUTPUTAMOUNT;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_NONE));
    // Stack should still have the 0 on it (NOP doesn't consume or push)
    BOOST_CHECK(stack.size() == 1);
}

BOOST_AUTO_TEST_SUITE_END()

// ==========================================
// Full covenant script integration tests
// ==========================================
BOOST_AUTO_TEST_SUITE(covenant_tests)

BOOST_AUTO_TEST_CASE(covenant_path_b_depositor_recovery)
{
    // Depositor can reclaim after locktime
    CKey depositorKey;
    depositorKey.MakeNewKey(true);

    int nLockTime = 26280;

    // Build covenant redeemScript (simplified — just Path B for this test)
    CScript redeemScript;
    redeemScript << CBigNum(nLockTime) << OP_CHECKLOCKTIMEVERIFY << OP_DROP;
    redeemScript << depositorKey.GetPubKey() << OP_CHECKSIG;

    // Create funding tx
    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].nValue = 100000;
    txFrom.vout[0].scriptPubKey = redeemScript;

    // Create spending tx with sufficient locktime
    CTransaction txTo = MakeSpendTx(txFrom, 100000, CScript() << OP_TRUE);
    txTo.nLockTime = 30000; // > 26280

    // Sign the transaction
    uint256 hash = SignatureHash(redeemScript, txTo, 0, SIGHASH_ALL);
    vector<unsigned char> vchSig;
    BOOST_CHECK(depositorKey.Sign(hash, vchSig));
    vchSig.push_back((unsigned char)SIGHASH_ALL);

    txTo.vin[0].scriptSig = CScript() << vchSig;

    // Verify with covenants active
    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, txTo.vin[0].scriptSig, txTo, 0, SIGHASH_ALL, SCRIPT_VERIFY_COVENANTS));
    BOOST_CHECK(EvalScript(stack, redeemScript, txTo, 0, SIGHASH_ALL, SCRIPT_VERIFY_COVENANTS));
    BOOST_CHECK(!stack.empty());
    BOOST_CHECK(CastToBool(stack.back()));
}

BOOST_AUTO_TEST_CASE(covenant_path_b_too_early)
{
    CKey depositorKey;
    depositorKey.MakeNewKey(true);

    CScript redeemScript;
    redeemScript << CBigNum(26280) << OP_CHECKLOCKTIMEVERIFY << OP_DROP;
    redeemScript << depositorKey.GetPubKey() << OP_CHECKSIG;

    CTransaction txFrom;
    txFrom.vout.resize(1);
    txFrom.vout[0].nValue = 100000;
    txFrom.vout[0].scriptPubKey = redeemScript;

    CTransaction txTo = MakeSpendTx(txFrom, 100000, CScript() << OP_TRUE);
    txTo.nLockTime = 20000; // Too early

    uint256 hash = SignatureHash(redeemScript, txTo, 0, SIGHASH_ALL);
    vector<unsigned char> vchSig;
    BOOST_CHECK(depositorKey.Sign(hash, vchSig));
    vchSig.push_back((unsigned char)SIGHASH_ALL);

    txTo.vin[0].scriptSig = CScript() << vchSig;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, txTo.vin[0].scriptSig, txTo, 0, SIGHASH_ALL, SCRIPT_VERIFY_COVENANTS));
    // CLTV should fail because locktime too low
    BOOST_CHECK(!EvalScript(stack, redeemScript, txTo, 0, SIGHASH_ALL, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(covenant_output_amount_verification)
{
    // Test that OP_OUTPUTAMOUNT + OP_EQUALVERIFY correctly validates output values
    CScript scriptPubKey;
    scriptPubKey << OP_0 << OP_OUTPUTAMOUNT << CBigNum(500 * COIN) << OP_EQUALVERIFY << OP_TRUE;

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);
    tx.vout[0].nValue = 500 * COIN;
    tx.vout[0].scriptPubKey = CScript() << OP_TRUE;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));

    // Wrong amount should fail
    tx.vout[0].nValue = 499 * COIN;
    stack.clear();
    BOOST_CHECK(!EvalScript(stack, scriptPubKey, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_CASE(covenant_output_script_hash_verification)
{
    // Test that OP_OUTPUTSCRIPT + OP_HASH160 + OP_EQUALVERIFY validates destinations
    CKey bobKey;
    bobKey.MakeNewKey(true);
    CKeyID bobAddr = bobKey.GetPubKey().GetID();

    CScript bobScriptPubKey;
    bobScriptPubKey << OP_DUP << OP_HASH160 << bobAddr << OP_EQUALVERIFY << OP_CHECKSIG;

    // Hash160 of the scriptPubKey
    uint160 scriptHash = Hash160(
        vector<unsigned char>(bobScriptPubKey.begin(), bobScriptPubKey.end()));

    CScript covenantScript;
    covenantScript << OP_0 << OP_OUTPUTSCRIPT << OP_HASH160
                   << scriptHash << OP_EQUALVERIFY << OP_TRUE;

    CTransaction tx;
    tx.vin.resize(1);
    tx.vout.resize(1);
    tx.vout[0].nValue = 50000;
    tx.vout[0].scriptPubKey = bobScriptPubKey;

    vector<vector<unsigned char> > stack;
    BOOST_CHECK(EvalScript(stack, covenantScript, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));

    // Wrong destination should fail
    CKey wrongKey;
    wrongKey.MakeNewKey(true);
    CScript wrongScript;
    wrongScript << OP_DUP << OP_HASH160 << wrongKey.GetPubKey().GetID() << OP_EQUALVERIFY << OP_CHECKSIG;
    tx.vout[0].scriptPubKey = wrongScript;

    stack.clear();
    BOOST_CHECK(!EvalScript(stack, covenantScript, tx, 0, 0, SCRIPT_VERIFY_COVENANTS));
}

BOOST_AUTO_TEST_SUITE_END()
