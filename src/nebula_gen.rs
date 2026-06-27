// ============================================================
// nebula_gen.rs — Fix #170: Comprehensive Input Validation
// Branch: fix/nebula-input-validation
// ============================================================
//
// Changes:
//   • Validate ship_id > 0 (must be a positive u64)
//   • Validate region_id is within acceptable range [1, MAX_REGION_ID]
//   • Added tests for edge cases and invalid inputs
//
// Note: Storage bloat / unexpected-behaviour risk is eliminated
// by rejecting invalid IDs before any storage write occurs.

#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, contracterror, log, symbol_short,
                   Address, BytesN, Env, Vec};

// ── Constants ────────────────────────────────────────────────
/// Maximum valid region ID.  Keeps storage bounded (issue #170 note).
pub const MAX_REGION_ID: u64 = 1_000_000;
/// Minimum valid ship ID (must be > 0).
pub const MIN_SHIP_ID: u64 = 1;
/// Default number of anomalies generated per layout.
pub const DEFAULT_ANOMALY_COUNT: u32 = 16;

// Salt constants for derive()
const SALT_X: u64 = 0x9e37_79b9_7f4a_7c15;
const SALT_Y: u64 = 0x6c62_272e_07bb_0142;
const SALT_R: u64 = 0xbf58_476d_1ce4_e5b9;
const SALT_T: u64 = 0x94d0_49bb_1331_11eb;

// ── Error enum ───────────────────────────────────────────────
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NebulaGenError {
    /// ship_id must be greater than zero.
    InvalidShipId    = 1,
    /// region_id must be between 1 and MAX_REGION_ID (inclusive).
    InvalidRegionId  = 2,
    /// seed cannot be all-zero bytes.
    InvalidSeed      = 3,
    /// No layout has been generated for this ship yet.
    LayoutNotFound   = 4,
    /// Anomaly index is out of bounds for this layout.
    AnomalyOutOfBounds = 5,
}

// ── Data types ───────────────────────────────────────────────
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceClass {
    Sparse,
    Moderate,
    Abundant,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Anomaly {
    pub x:              u64,
    pub y:              u64,
    pub rarity:         u64,
    pub anomaly_type:   u64,
    pub resource_class: ResourceClass,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct NebulaLayout {
    pub ship_id:     u64,
    pub region_id:   u64,
    pub layout_hash: BytesN<32>,
    pub anomalies:   Vec<Anomaly>,
    pub size:        u32,
}

#[contracttype]
pub enum DataKey {
    ActiveLayout(u64), // keyed by ship_id
}

// ── Contract ─────────────────────────────────────────────────
#[contract]
pub struct NebulaGenContract;

#[contractimpl]
impl NebulaGenContract {

    // ── generate_nebula_layout ────────────────────────────────
    /// Generate a deterministic nebula layout for a given ship / region.
    ///
    /// # Validation (Issue #170)
    /// - `ship_id`   must be > 0
    /// - `region_id` must be in [1, MAX_REGION_ID]
    /// - `seed`      must not be all-zero
    pub fn generate_nebula_layout(
        env:       &Env,
        caller:    Address,
        ship_id:   u64,
        region_id: u64,
        seed:      BytesN<32>,
    ) -> Result<NebulaLayout, NebulaGenError> {
        // ── Require caller authentication ─────────────────────
        caller.require_auth();

        // ── Input validation (Issue #170) ─────────────────────
        // ship_id must be > 0
        if ship_id < MIN_SHIP_ID {
            log!(env, "NebulaGen: invalid ship_id={}", ship_id);
            return Err(NebulaGenError::InvalidShipId);
        }

        // region_id must be in [1, MAX_REGION_ID]
        if region_id < 1 || region_id > MAX_REGION_ID {
            log!(env, "NebulaGen: invalid region_id={}", region_id);
            return Err(NebulaGenError::InvalidRegionId);
        }

        // Seed must not be all-zero
        let seed_bytes = seed.to_array();
        if seed_bytes.iter().all(|&b| b == 0) {
            return Err(NebulaGenError::InvalidSeed);
        }

        // ── Build entropy master ──────────────────────────────
        let ledger_seq  = env.ledger().sequence() as u64;
        let timestamp   = env.ledger().timestamp();

        // Convert seed to u64 for mixing (first 8 bytes)
        let seed_u64 = u64::from_be_bytes(seed_bytes[0..8].try_into().unwrap_or([0u8; 8]));

        let master: u64 = splitmix64(seed_u64)
            ^ splitmix64(ledger_seq)
            ^ splitmix64(timestamp)
            ^ splitmix64(ship_id)
            ^ splitmix64(region_id);

        // ── Generate anomalies ────────────────────────────────
        let mut anomalies = Vec::new(env);
        for i in 0..DEFAULT_ANOMALY_COUNT {
            let idx = i as u64;
            let x          = derive(master, idx, SALT_X) % 1000;
            let y          = derive(master, idx, SALT_Y) % 1000;
            let rarity     = derive(master, idx, SALT_R) % 101;
            let anom_type  = derive(master, idx, SALT_T) % 5;

            let resource_class = match rarity {
                0..=33  => ResourceClass::Sparse,
                34..=66 => ResourceClass::Moderate,
                _       => ResourceClass::Abundant,
            };

            anomalies.push_back(Anomaly { x, y, rarity, anomaly_type: anom_type, resource_class });
        }

        // ── Build layout hash from master ─────────────────────
        let hash_bytes = master.to_be_bytes();
        let mut hash_arr = [0u8; 32];
        // Fill deterministically from master
        for (i, chunk) in hash_arr.chunks_mut(8).enumerate() {
            let val = splitmix64(master.wrapping_add(i as u64));
            chunk.copy_from_slice(&val.to_be_bytes());
        }
        let layout_hash = BytesN::from_array(env, &hash_arr);

        let layout = NebulaLayout {
            ship_id,
            region_id,
            layout_hash: layout_hash.clone(),
            anomalies: anomalies.clone(),
            size: DEFAULT_ANOMALY_COUNT,
        };

        // ── Persist layout ────────────────────────────────────
        env.storage()
            .persistent()
            .set(&DataKey::ActiveLayout(ship_id), &layout);

        // ── Emit event ────────────────────────────────────────
        env.events().publish(
            (symbol_short!("NebulaGen"), symbol_short!("generated")),
            (ship_id, layout_hash, DEFAULT_ANOMALY_COUNT),
        );

        log!(env, "NebulaGen: generated layout ship_id={} region_id={} size={}",
             ship_id, region_id, DEFAULT_ANOMALY_COUNT);

        Ok(layout)
    }

    /// Check whether an anomaly exists at `anomaly_index` for `ship_id`.
    pub fn has_anomaly(
        env:           &Env,
        ship_id:       u64,
        anomaly_index: u32,
    ) -> Result<bool, NebulaGenError> {
        // Validate ship_id even for read operations
        if ship_id < MIN_SHIP_ID {
            return Err(NebulaGenError::InvalidShipId);
        }

        let layout: NebulaLayout = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLayout(ship_id))
            .ok_or(NebulaGenError::LayoutNotFound)?;

        if anomaly_index >= layout.size {
            return Err(NebulaGenError::AnomalyOutOfBounds);
        }

        Ok(true)
    }
}

// ── PRNG helpers ─────────────────────────────────────────────

/// SplitMix64 — bijective, avalanche-quality finaliser.
#[inline]
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Derive a deterministic value from (master, index, salt).
#[inline]
fn derive(master: u64, index: u64, salt: u64) -> u64 {
    splitmix64(master ^ splitmix64(index.wrapping_add(salt)))
}

// ─────────────────────────────────────────────────────────────
// Tests — Issue #170: edge cases and invalid inputs
// ─────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

    fn make_env() -> Env {
        let env = Env::default();
        env.mock_all_auths();
        env
    }

    fn zero_seed(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }

    fn valid_seed(env: &Env) -> BytesN<32> {
        BytesN::from_array(env, &[1u8, 2, 3, 4, 5, 6, 7, 8,
                                    9, 10, 11, 12, 13, 14, 15, 16,
                                    17, 18, 19, 20, 21, 22, 23, 24,
                                    25, 26, 27, 28, 29, 30, 31, 32])
    }

    // ── ship_id validation ────────────────────────────────────

    #[test]
    fn test_ship_id_zero_rejected() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 0, 1, valid_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidShipId));
    }

    #[test]
    fn test_ship_id_one_accepted() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, 1, valid_seed(&env),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_ship_id_max_u64_accepted() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, u64::MAX, 1, valid_seed(&env),
        );
        // ship_id = u64::MAX is > 0, valid per spec
        assert!(result.is_ok());
    }

    // ── region_id validation ──────────────────────────────────

    #[test]
    fn test_region_id_zero_rejected() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, 0, valid_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidRegionId));
    }

    #[test]
    fn test_region_id_one_accepted() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, 1, valid_seed(&env),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_region_id_max_accepted() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, MAX_REGION_ID, valid_seed(&env),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_region_id_exceeds_max_rejected() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, MAX_REGION_ID + 1, valid_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidRegionId));
    }

    #[test]
    fn test_region_id_u64_max_rejected() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, u64::MAX, valid_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidRegionId));
    }

    // ── seed validation ───────────────────────────────────────

    #[test]
    fn test_all_zero_seed_rejected() {
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 1, 1, zero_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidSeed));
    }

    // ── combined invalid inputs ───────────────────────────────

    #[test]
    fn test_both_ids_invalid_returns_ship_id_error_first() {
        // ship_id is checked before region_id
        let env    = make_env();
        let caller = Address::generate(&env);
        let result = NebulaGenContract::generate_nebula_layout(
            &env, caller, 0, 0, valid_seed(&env),
        );
        assert_eq!(result, Err(NebulaGenError::InvalidShipId));
    }

    // ── has_anomaly validation ────────────────────────────────

    #[test]
    fn test_has_anomaly_ship_id_zero_rejected() {
        let env    = make_env();
        let result = NebulaGenContract::has_anomaly(&env, 0, 0);
        assert_eq!(result, Err(NebulaGenError::InvalidShipId));
    }

    #[test]
    fn test_has_anomaly_layout_not_found() {
        let env    = make_env();
        // ship_id=99 — no layout stored
        let result = NebulaGenContract::has_anomaly(&env, 99, 0);
        assert_eq!(result, Err(NebulaGenError::LayoutNotFound));
    }

    #[test]
    fn test_has_anomaly_out_of_bounds() {
        let env    = make_env();
        let caller = Address::generate(&env);
        // Generate a layout first
        NebulaGenContract::generate_nebula_layout(
            &env, caller, 5, 1, valid_seed(&env),
        ).unwrap();
        // DEFAULT_ANOMALY_COUNT = 16, so index 16 is OOB
        let result = NebulaGenContract::has_anomaly(&env, 5, DEFAULT_ANOMALY_COUNT);
        assert_eq!(result, Err(NebulaGenError::AnomalyOutOfBounds));
    }

    #[test]
    fn test_has_anomaly_valid() {
        let env    = make_env();
        let caller = Address::generate(&env);
        NebulaGenContract::generate_nebula_layout(
            &env, caller, 5, 1, valid_seed(&env),
        ).unwrap();
        let result = NebulaGenContract::has_anomaly(&env, 5, 0);
        assert_eq!(result, Ok(true));
    }

    // ── determinism check ─────────────────────────────────────

    #[test]
    fn test_same_inputs_produce_same_layout_hash() {
        let env     = make_env();
        let caller1 = Address::generate(&env);
        let caller2 = Address::generate(&env);
        let seed    = valid_seed(&env);

        // Ledger sequence is the same for both calls in the test env
        let layout1 = NebulaGenContract::generate_nebula_layout(
            &env, caller1, 42, 100, seed.clone(),
        ).unwrap();
        let layout2 = NebulaGenContract::generate_nebula_layout(
            &env, caller2, 42, 100, seed,
        ).unwrap();

        assert_eq!(layout1.layout_hash, layout2.layout_hash);
    }
}
