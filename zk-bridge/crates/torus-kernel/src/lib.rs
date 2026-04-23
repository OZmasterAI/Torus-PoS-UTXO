use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};
use std::cmp::Ordering;

pub const COIN: u64 = 100_000_000;
pub const STAKE_MIN_AGE: u32 = 8 * 60 * 60;
pub const STAKE_MAX_AGE: u64 = 90 * 24 * 60 * 60;
pub const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeKernelInput {
    pub n_bits: u32,
    pub stake_modifier: u64,
    pub block_time_from: u32,
    pub tx_prev_offset: u32,
    pub tx_prev_time: u32,
    pub prevout_n: u32,
    pub time_tx: u32,
    pub value_in: u64,
    pub is_permanent_stake: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeKernelOutput {
    pub hash_proof_of_stake: [u8; 32],
    pub meets_target: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub hash_prev_block: [u8; 32],
    pub hash_merkle_root: [u8; 32],
    pub time: u32,
    pub bits: u32,
    pub nonce: u32,
}

// --- U256 arithmetic (little-endian [u64; 4]) ---

fn bytes_to_limbs(bytes: &[u8; 32]) -> [u64; 4] {
    let mut limbs = [0u64; 4];
    for i in 0..4 {
        let off = i * 8;
        limbs[i] = u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap());
    }
    limbs
}


fn cmp_u256(a: &[u64; 4], b: &[u64; 4]) -> Ordering {
    for i in (0..4).rev() {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// u64 × U256 → (low 256 bits, overflow word).
/// If overflow > 0, the product exceeds 2^256.
fn mul_u64_u256(a: u64, b: &[u64; 4]) -> ([u64; 4], u64) {
    let mut result = [0u64; 4];
    let mut carry = 0u128;
    for i in 0..4 {
        let prod = (a as u128) * (b[i] as u128) + carry;
        result[i] = prod as u64;
        carry = prod >> 64;
    }
    (result, carry as u64)
}

// --- Core functions ---

/// Decode Bitcoin/PPCoin compact target (nBits) to 256-bit LE representation.
///
/// Format: top byte = exponent, lower 3 bytes = mantissa (bit 23 = sign).
/// target = mantissa × 2^(8×(exponent−3))
pub fn compact_to_target(n_bits: u32) -> [u8; 32] {
    let mut target = [0u8; 32];
    let n_size = (n_bits >> 24) as usize;
    let negative = (n_bits & 0x0080_0000) != 0;
    let n_word = n_bits & 0x007f_ffff;

    if negative || n_size == 0 || n_word == 0 {
        return target;
    }

    if n_size <= 3 {
        let value = n_word >> (8 * (3 - n_size));
        for i in 0..n_size.min(32) {
            target[i] = ((value >> (8 * i)) & 0xFF) as u8;
        }
    } else {
        let offset = n_size - 3;
        for i in 0..3 {
            if offset + i < 32 {
                target[offset + i] = ((n_word >> (8 * i)) & 0xFF) as u8;
            }
        }
    }

    target
}

/// Double SHA-256 (matches Bitcoin/PPCoin Hash() function).
///
/// The output bytes use the same memory layout as Bitcoin's uint256:
/// SHA-256 byte\[0\] (digest MSB) → result byte\[0\] (uint256 LSB).
pub fn double_sha256(data: &[u8]) -> [u8; 32] {
    let h1 = Sha256::digest(data);
    Sha256::digest(&h1).into()
}

/// Compute staking time weight (port of GetWeight from kernel.cpp).
///
/// Weight = clamp(elapsed − minAge, 0, maxAge).
/// Permanent stakes get 4× the age cap (360 days vs 90 days).
pub fn get_weight(interval_beginning: u64, interval_end: u64, permanent_stake: bool) -> u64 {
    let raw = interval_end
        .saturating_sub(interval_beginning)
        .saturating_sub(STAKE_MIN_AGE as u64);
    let max_age = if permanent_stake {
        STAKE_MAX_AGE * 4
    } else {
        STAKE_MAX_AGE
    };
    raw.min(max_age)
}

/// Check stake kernel hash against target (port of CheckStakeKernelHash from kernel.cpp).
///
/// Kernel formula:
///   hash(stakeModifier ‖ blockTime ‖ txOffset ‖ txPrevTime ‖ prevoutN ‖ timeTx)
///     < target_per_coin_day × coinDayWeight
///
/// The hash input is 28 bytes, serialized little-endian (matching CDataStream SER_GETHASH).
pub fn check_stake_kernel_hash(input: &StakeKernelInput) -> StakeKernelOutput {
    assert!(input.time_tx >= input.tx_prev_time, "nTime violation");
    assert!(
        input.block_time_from + STAKE_MIN_AGE <= input.time_tx,
        "min age violation"
    );

    // Decode compact target
    let target_bytes = compact_to_target(input.n_bits);
    let target_limbs = bytes_to_limbs(&target_bytes);

    // Coin day weight = value_in × weight / COIN / 86400
    let weight = get_weight(
        input.tx_prev_time as u64,
        input.time_tx as u64,
        input.is_permanent_stake,
    );
    let coin_day_weight =
        ((input.value_in as u128) * (weight as u128) / (COIN as u128) / (SECONDS_PER_DAY as u128))
            as u64;

    // Serialize kernel hash input (28 bytes, little-endian)
    let mut data = Vec::with_capacity(28);
    data.extend_from_slice(&input.stake_modifier.to_le_bytes());
    data.extend_from_slice(&input.block_time_from.to_le_bytes());
    data.extend_from_slice(&input.tx_prev_offset.to_le_bytes());
    data.extend_from_slice(&input.tx_prev_time.to_le_bytes());
    data.extend_from_slice(&input.prevout_n.to_le_bytes());
    data.extend_from_slice(&input.time_tx.to_le_bytes());

    let hash = double_sha256(&data);
    let hash_limbs = bytes_to_limbs(&hash);

    // Compare: hash <= coinDayWeight × targetPerCoinDay
    let (product_low, overflow) = mul_u64_u256(coin_day_weight, &target_limbs);
    let meets_target = if overflow > 0 {
        true // product exceeds 2^256, any hash passes
    } else {
        cmp_u256(&hash_limbs, &product_low) != Ordering::Greater
    };

    StakeKernelOutput {
        hash_proof_of_stake: hash,
        meets_target,
    }
}

/// Hash a block header (double SHA-256 of the 80-byte serialized header).
pub fn hash_block_header(header: &BlockHeader) -> [u8; 32] {
    let mut data = Vec::with_capacity(80);
    data.extend_from_slice(&header.version.to_le_bytes());
    data.extend_from_slice(&header.hash_prev_block);
    data.extend_from_slice(&header.hash_merkle_root);
    data.extend_from_slice(&header.time.to_le_bytes());
    data.extend_from_slice(&header.bits.to_le_bytes());
    data.extend_from_slice(&header.nonce.to_le_bytes());
    double_sha256(&data)
}

/// Verify that a chain of block headers links correctly via hashPrevBlock.
pub fn verify_header_chain(headers: &[BlockHeader]) -> bool {
    if headers.is_empty() {
        return false;
    }
    for i in 1..headers.len() {
        let prev_hash = hash_block_header(&headers[i - 1]);
        if prev_hash != headers[i].hash_prev_block {
            return false;
        }
    }
    true
}

/// Verify a Merkle inclusion proof (double-SHA256 based).
pub fn verify_merkle_proof(tx_hash: &[u8; 32], proof: &[([u8; 32], bool)], root: &[u8; 32]) -> bool {
    let mut current = *tx_hash;
    for (sibling, is_right) in proof {
        let mut combined = Vec::with_capacity(64);
        if *is_right {
            combined.extend_from_slice(&current);
            combined.extend_from_slice(sibling);
        } else {
            combined.extend_from_slice(sibling);
            combined.extend_from_slice(&current);
        }
        current = double_sha256(&combined);
    }
    current == *root
}

// --- Display helpers ---

/// Format a uint256 hash as hex string (reversed byte order, matching Bitcoin display).
pub fn hash_to_hex(hash: &[u8; 32]) -> String {
    let mut reversed = *hash;
    reversed.reverse();
    reversed.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_to_target_genesis() {
        // Bitcoin genesis nBits = 0x1d00ffff
        let target = compact_to_target(0x1d00ffff);
        // Expected: 0x00000000FFFF0000...0
        // In LE bytes: byte[26]=0xFF, byte[27]=0xFF, rest zero
        assert_eq!(target[26], 0xFF);
        assert_eq!(target[27], 0xFF);
        assert_eq!(target[28], 0x00);
        for i in 0..26 {
            assert_eq!(target[i], 0x00, "byte {} should be zero", i);
        }
    }

    #[test]
    fn test_compact_to_target_small() {
        // nBits with nSize = 3
        let target = compact_to_target(0x03123456);
        // mantissa = 0x123456, nSize = 3
        // Stored at bytes 0-2 (offset = 0)
        assert_eq!(target[0], 0x56);
        assert_eq!(target[1], 0x34);
        assert_eq!(target[2], 0x12);
    }

    #[test]
    fn test_double_sha256_empty() {
        let hash = double_sha256(b"");
        let hex: String = hash.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(
            hex,
            "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456"
        );
    }

    #[test]
    fn test_get_weight_normal() {
        let begin = 1000000u64;
        let end = begin + STAKE_MIN_AGE as u64 + 86400; // +1 day after min age
        let w = get_weight(begin, end, false);
        assert_eq!(w, 86400); // 1 day
    }

    #[test]
    fn test_get_weight_capped() {
        let begin = 1000000u64;
        let end = begin + STAKE_MIN_AGE as u64 + STAKE_MAX_AGE + 10000;
        let w = get_weight(begin, end, false);
        assert_eq!(w, STAKE_MAX_AGE); // capped at 90 days
    }

    #[test]
    fn test_get_weight_permanent_stake() {
        let begin = 1000000u64;
        let end = begin + STAKE_MIN_AGE as u64 + STAKE_MAX_AGE * 4 + 10000;
        let w = get_weight(begin, end, true);
        assert_eq!(w, STAKE_MAX_AGE * 4); // capped at 360 days
    }

    #[test]
    fn test_kernel_hash_serialization() {
        // Verify the 28-byte serialization matches expected layout
        let input = StakeKernelInput {
            n_bits: 0x1d00ffff,
            stake_modifier: 0x0102030405060708,
            block_time_from: 0x11223344,
            tx_prev_offset: 0x55667788,
            tx_prev_time: 0x99aabb00, // must be <= time_tx
            prevout_n: 0xccdd0000,
            time_tx: 0xeeff0000,
            value_in: 100 * COIN,
            is_permanent_stake: false,
        };

        let mut data = Vec::with_capacity(28);
        data.extend_from_slice(&input.stake_modifier.to_le_bytes());
        data.extend_from_slice(&input.block_time_from.to_le_bytes());
        data.extend_from_slice(&input.tx_prev_offset.to_le_bytes());
        data.extend_from_slice(&input.tx_prev_time.to_le_bytes());
        data.extend_from_slice(&input.prevout_n.to_le_bytes());
        data.extend_from_slice(&input.time_tx.to_le_bytes());
        assert_eq!(data.len(), 28);

        // First 8 bytes = stake_modifier LE
        assert_eq!(data[0], 0x08);
        assert_eq!(data[7], 0x01);
        // Bytes 8-11 = block_time_from LE
        assert_eq!(data[8], 0x44);
        assert_eq!(data[11], 0x11);
    }

    #[test]
    fn test_mul_u64_u256_no_overflow() {
        let a = 100u64;
        let b = [1u64, 0, 0, 0];
        let (result, overflow) = mul_u64_u256(a, &b);
        assert_eq!(result, [100, 0, 0, 0]);
        assert_eq!(overflow, 0);
    }

    #[test]
    fn test_mul_u64_u256_with_overflow() {
        let a = u64::MAX;
        let b = [0, 0, 0, u64::MAX];
        let (_, overflow) = mul_u64_u256(a, &b);
        assert!(overflow > 0);
    }

    #[test]
    fn test_cmp_u256() {
        let a = [1u64, 0, 0, 0];
        let b = [2u64, 0, 0, 0];
        assert_eq!(cmp_u256(&a, &b), Ordering::Less);

        let c = [0u64, 0, 0, 1];
        assert_eq!(cmp_u256(&a, &c), Ordering::Less);
        assert_eq!(cmp_u256(&c, &a), Ordering::Greater);
    }

    #[test]
    fn test_verify_header_chain() {
        let h0 = BlockHeader {
            version: 1,
            hash_prev_block: [0u8; 32],
            hash_merkle_root: [0xAA; 32],
            time: 1000,
            bits: 0x1d00ffff,
            nonce: 42,
        };
        let h0_hash = hash_block_header(&h0);

        let h1 = BlockHeader {
            version: 1,
            hash_prev_block: h0_hash,
            hash_merkle_root: [0xBB; 32],
            time: 1120,
            bits: 0x1d00ffff,
            nonce: 99,
        };

        assert!(verify_header_chain(&[h0.clone(), h1.clone()]));

        // Break the chain
        let h1_bad = BlockHeader {
            hash_prev_block: [0xFF; 32],
            ..h1
        };
        assert!(!verify_header_chain(&[h0, h1_bad]));
    }

    #[test]
    fn test_check_stake_kernel_hash_deterministic() {
        let input = StakeKernelInput {
            n_bits: 0x1d00ffff,
            stake_modifier: 0xDEADBEEF,
            block_time_from: 1700000000,
            tx_prev_offset: 100,
            tx_prev_time: 1700000000,
            prevout_n: 0,
            time_tx: 1700000000 + STAKE_MIN_AGE + 86400,
            value_in: 1000 * COIN,
            is_permanent_stake: false,
        };
        let output = check_stake_kernel_hash(&input);
        assert_ne!(output.hash_proof_of_stake, [0u8; 32]);
        let output2 = check_stake_kernel_hash(&input);
        assert_eq!(output.hash_proof_of_stake, output2.hash_proof_of_stake);
    }

    #[test]
    fn test_kernel_hash_meets_easy_target() {
        let input = StakeKernelInput {
            n_bits: 0x207fffff,
            stake_modifier: 0x1234,
            block_time_from: 1700000000,
            tx_prev_offset: 200,
            tx_prev_time: 1700000000,
            prevout_n: 0,
            time_tx: 1700000000 + STAKE_MIN_AGE + STAKE_MAX_AGE as u32,
            value_in: 50_000_000 * COIN,
            is_permanent_stake: false,
        };
        let output = check_stake_kernel_hash(&input);
        assert!(output.meets_target);
    }

    #[test]
    fn test_kernel_hash_fails_tiny_target() {
        let input = StakeKernelInput {
            n_bits: 0x03000001,
            stake_modifier: 0xABCD,
            block_time_from: 1700000000,
            tx_prev_offset: 50,
            tx_prev_time: 1700000000,
            prevout_n: 0,
            time_tx: 1700000000 + STAKE_MIN_AGE + 86400,
            value_in: 100 * COIN,
            is_permanent_stake: false,
        };
        let output = check_stake_kernel_hash(&input);
        assert!(!output.meets_target);
    }

    #[test]
    fn test_verify_merkle_proof() {
        let leaf = double_sha256(b"tx_data");
        let sibling = double_sha256(b"other_tx");

        // Compute root: hash(leaf || sibling)
        let mut combined = Vec::with_capacity(64);
        combined.extend_from_slice(&leaf);
        combined.extend_from_slice(&sibling);
        let root = double_sha256(&combined);

        // Proof: sibling is on the right
        let proof = vec![(sibling, true)];
        assert!(verify_merkle_proof(&leaf, &proof, &root));

        // Wrong root should fail
        let bad_root = [0xFFu8; 32];
        assert!(!verify_merkle_proof(&leaf, &proof, &bad_root));
    }
}

#[cfg(test)]
mod genesis_tests {
    use super::*;

    fn hex_to_internal(hex_str: &str) -> [u8; 32] {
        let bytes: Vec<u8> = (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..i+2], 16).unwrap())
            .collect();
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = bytes[31 - i];
        }
        result
    }

    #[test]
    fn test_genesis_block_hash() {
        let header = BlockHeader {
            version: 1,
            hash_prev_block: [0u8; 32],
            hash_merkle_root: hex_to_internal(
                "cdc376c01136ce03cbdf5c6faa1eeaaddaff9d2e40a4fdb2825b1cff8e123de6"
            ),
            time: 1638617750,
            bits: 0x1e0fffff,
            nonce: 627293,
        };
        let hash = hash_block_header(&header);
        let display_hex = hash_to_hex(&hash);
        println!("Computed genesis hash: {}", display_hex);
        assert_eq!(
            display_hex,
            "000005a39de532e9f2546ad8c954a21f01e0064f3edc9fea108f39e0499a011d"
        );
    }
}
