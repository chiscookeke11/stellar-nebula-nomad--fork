// ============================================================
// event_index.rs — Fix #177: Proper indexing tags for all events
// Branch: feat/event-indexing
// ============================================================
//
// Problem: events were emitted without indexed topics, making
// off-chain queries (Horizon event stream, custom indexers,
// analytics) O(n) scans over all events.
//
// Solution: use a two-element topics tuple as the first arg to
// env.events().publish().  Soroban indexes both topic elements,
// so indexers can efficiently filter on (contract_type, event_type)
// or on (event_type, entity_id) patterns.
//
// This file contains:
//   1.  Canonical topic constants for every event emitted by the
//       contract suite.
//   2.  Typed emit_* helper functions that centralise event
//       construction and make it impossible to forget indexing.
//   3.  Updated event schemas (as doc-comments) for API docs.
//   4.  Tests that verify events are published with the expected
//       indexed topics.

#![no_std]
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

// ─────────────────────────────────────────────────────────────
// Topic constants  (index-0 = contract domain, index-1 = action)
// Soroban's event filter can match on topics[0] and/or topics[1].
// ─────────────────────────────────────────────────────────────

// ── Nebula Explorer domain ────────────────────────────────────
pub const TOPIC_NEBULA: Symbol       = symbol_short!("Nebula");
pub const ACTION_SCANNED: Symbol     = symbol_short!("scanned");   // scan_nebula()
pub const ACTION_GENERATED: Symbol   = symbol_short!("generated"); // generate_nebula_layout()

// ── Resource Minter domain ────────────────────────────────────
pub const TOPIC_MINTER: Symbol       = symbol_short!("Minter");
pub const ACTION_MINTED: Symbol      = symbol_short!("minted");    // mint_resource()
pub const ACTION_TRANSFERRED: Symbol = symbol_short!("transfer");  // future: transfer

// ── Ship Registry domain ──────────────────────────────────────
pub const TOPIC_SHIP: Symbol         = symbol_short!("Ship");
pub const ACTION_REGISTERED: Symbol  = symbol_short!("register");  // register_ship()
pub const ACTION_UPGRADED: Symbol    = symbol_short!("upgraded");   // upgrade_ship()

// ── Nomad Bonding domain ──────────────────────────────────────
pub const TOPIC_BOND: Symbol         = symbol_short!("Bond");
pub const ACTION_CREATED: Symbol     = symbol_short!("created");   // create_bond()
pub const ACTION_ACCEPTED: Symbol    = symbol_short!("accepted");  // accept_bond()
pub const ACTION_DELEGATED: Symbol   = symbol_short!("delegated"); // delegate_yield()
pub const ACTION_CLAIMED: Symbol     = symbol_short!("claimed");   // claim_yield()
pub const ACTION_DISSOLVED: Symbol   = symbol_short!("dissolved"); // dissolve_bond()

// ── Rate Limiter domain ───────────────────────────────────────
pub const TOPIC_RATE: Symbol         = symbol_short!("RateLimit");
pub const ACTION_HIT: Symbol         = symbol_short!("hit");       // rate limit hit

// ─────────────────────────────────────────────────────────────
// Typed emit helpers
// Each function documents its event schema inline (for API docs).
// ─────────────────────────────────────────────────────────────

// ── Nebula Explorer events ────────────────────────────────────

/// Emitted by `scan_nebula()`.
///
/// ```
/// Event schema — NebulaScanned
/// topics : [Symbol("Nebula"), Symbol("scanned")]
/// data   : { region_id: u64, layout_hash: BytesN<32>, rarity_score: u32 }
///
/// Indexers: filter on topics[0]="Nebula" AND topics[1]="scanned"
/// ```
pub fn emit_nebula_scanned(
    env:          &Env,
    region_id:    u64,
    layout_hash:  BytesN<32>,
    rarity_score: u32,
) {
    env.events().publish(
        (TOPIC_NEBULA, ACTION_SCANNED),          // ← indexed topics tuple
        (region_id, layout_hash, rarity_score),  // ← unindexed data payload
    );
}

/// Emitted by `generate_nebula_layout()`.
///
/// ```
/// Event schema — NebulaGenerated
/// topics : [Symbol("Nebula"), Symbol("generated")]
/// data   : { ship_id: u64, layout_hash: BytesN<32>, size: u32 }
/// ```
pub fn emit_nebula_generated(
    env:         &Env,
    ship_id:     u64,
    layout_hash: BytesN<32>,
    size:        u32,
) {
    env.events().publish(
        (TOPIC_NEBULA, ACTION_GENERATED),
        (ship_id, layout_hash, size),
    );
}

// ── Resource Minter events ────────────────────────────────────

/// Emitted by `mint_resource()`.
///
/// ```
/// Event schema — ResourceMinted
/// topics : [Symbol("Minter"), Symbol("minted")]
/// data   : { owner: Address, resource_type: ResourceType, amount: u64 }
///
/// Indexers: filter topics[0]="Minter" to get all mint events;
///           additionally match topics[1]="minted".
/// ```
pub fn emit_resource_minted(
    env:           &Env,
    owner:         Address,
    resource_type: impl soroban_sdk::TryIntoVal<Env, soroban_sdk::Val>,
    amount:        u64,
) {
    env.events().publish(
        (TOPIC_MINTER, ACTION_MINTED),
        (owner, resource_type, amount),
    );
}

// ── Ship Registry events ──────────────────────────────────────

/// Emitted by `register_ship()`.
///
/// ```
/// Event schema — ShipRegistered
/// topics : [Symbol("Ship"), Symbol("register")]
/// data   : { owner: Address, ship_id: u64, ship_name_hash: BytesN<32> }
/// ```
pub fn emit_ship_registered(
    env:            &Env,
    owner:          Address,
    ship_id:        u64,
    ship_name_hash: BytesN<32>,
) {
    env.events().publish(
        (TOPIC_SHIP, ACTION_REGISTERED),
        (owner, ship_id, ship_name_hash),
    );
}

/// Emitted by `upgrade_ship()`.
///
/// ```
/// Event schema — ShipUpgraded
/// topics : [Symbol("Ship"), Symbol("upgraded")]
/// data   : { owner: Address, ship_id: u64, new_level: u32 }
/// ```
pub fn emit_ship_upgraded(
    env:       &Env,
    owner:     Address,
    ship_id:   u64,
    new_level: u32,
) {
    env.events().publish(
        (TOPIC_SHIP, ACTION_UPGRADED),
        (owner, ship_id, new_level),
    );
}

// ── Nomad Bonding events ──────────────────────────────────────

/// Emitted by `create_bond()`.
///
/// ```
/// Event schema — BondCreated
/// topics : [Symbol("Bond"), Symbol("created")]
/// data   : { party_a: Address, party_b: Address, bond_id: u64 }
/// ```
pub fn emit_bond_created(
    env:     &Env,
    party_a: Address,
    party_b: Address,
    bond_id: u64,
) {
    env.events().publish(
        (TOPIC_BOND, ACTION_CREATED),
        (party_a, party_b, bond_id),
    );
}

/// Emitted by `accept_bond()`.
///
/// ```
/// Event schema — BondAccepted
/// topics : [Symbol("Bond"), Symbol("accepted")]
/// data   : { bond_id: u64, party_b: Address }
/// ```
pub fn emit_bond_accepted(env: &Env, bond_id: u64, party_b: Address) {
    env.events().publish(
        (TOPIC_BOND, ACTION_ACCEPTED),
        (bond_id, party_b),
    );
}

/// Emitted by `delegate_yield()`.
///
/// ```
/// Event schema — YieldDelegated
/// topics : [Symbol("Bond"), Symbol("delegated")]
/// data   : { bond_id: u64, delegator: Address, yield_percent: u32, amount: u64 }
/// ```
pub fn emit_yield_delegated(
    env:           &Env,
    bond_id:       u64,
    delegator:     Address,
    yield_percent: u32,
    amount:        u64,
) {
    env.events().publish(
        (TOPIC_BOND, ACTION_DELEGATED),
        (bond_id, delegator, yield_percent, amount),
    );
}

/// Emitted by `claim_yield()`.
///
/// ```
/// Event schema — YieldClaimed
/// topics : [Symbol("Bond"), Symbol("claimed")]
/// data   : { bond_id: u64, beneficiary: Address, amount: u64 }
/// ```
pub fn emit_yield_claimed(env: &Env, bond_id: u64, beneficiary: Address, amount: u64) {
    env.events().publish(
        (TOPIC_BOND, ACTION_CLAIMED),
        (bond_id, beneficiary, amount),
    );
}

/// Emitted by `dissolve_bond()`.
///
/// ```
/// Event schema — BondDissolved
/// topics : [Symbol("Bond"), Symbol("dissolved")]
/// data   : { bond_id: u64, initiator: Address }
/// ```
pub fn emit_bond_dissolved(env: &Env, bond_id: u64, initiator: Address) {
    env.events().publish(
        (TOPIC_BOND, ACTION_DISSOLVED),
        (bond_id, initiator),
    );
}

// ── Rate Limiter events ───────────────────────────────────────

/// Emitted when a rate limit is hit.
///
/// ```
/// Event schema — RateLimitHit
/// topics : [Symbol("RateLimit"), Symbol("hit")]
/// data   : { caller: Address, operation: Symbol, call_count: u32, max_calls: u32 }
///
/// Critical for analytics: filter topics[0]="RateLimit" to detect DoS attempts.
/// ```
pub fn emit_rate_limit_hit(
    env:        &Env,
    caller:     Address,
    operation:  Symbol,
    call_count: u32,
    max_calls:  u32,
) {
    env.events().publish(
        (TOPIC_RATE, ACTION_HIT),
        (caller, operation, call_count, max_calls),
    );
}

// ─────────────────────────────────────────────────────────────
// Tests — Issue #177
// ─────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, BytesN, Env, IntoVal, Val, Vec,
    };

    fn make_env() -> Env {
        Env::default()
    }

    // Helper: check that at least one event was published whose
    // topics start with (expected_topic0, expected_topic1).
    fn assert_event_published(env: &Env, topic0: Symbol, topic1: Symbol) {
        let events = env.events().all();
        let found = events.iter().any(|(_, topics, _)| {
            if let (Ok(t0), Ok(t1)) = (
                topics.get(0).map(|v| Symbol::try_from_val(env, &v)),
                topics.get(1).map(|v| Symbol::try_from_val(env, &v)),
            ) {
                t0 == Ok(topic0.clone()) && t1 == Ok(topic1.clone())
            } else {
                false
            }
        });
        assert!(
            found,
            "Expected event with topics ({:?}, {:?}) was not found",
            topic0, topic1
        );
    }

    #[test]
    fn test_nebula_scanned_event_has_indexed_topics() {
        let env  = make_env();
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        emit_nebula_scanned(&env, 42, hash, 150);
        assert_event_published(&env, TOPIC_NEBULA, ACTION_SCANNED);
    }

    #[test]
    fn test_nebula_generated_event_has_indexed_topics() {
        let env  = make_env();
        let hash = BytesN::from_array(&env, &[2u8; 32]);
        emit_nebula_generated(&env, 7, hash, 16);
        assert_event_published(&env, TOPIC_NEBULA, ACTION_GENERATED);
    }

    #[test]
    fn test_ship_registered_event_has_indexed_topics() {
        let env   = make_env();
        let owner = Address::generate(&env);
        let hash  = BytesN::from_array(&env, &[3u8; 32]);
        emit_ship_registered(&env, owner, 99, hash);
        assert_event_published(&env, TOPIC_SHIP, ACTION_REGISTERED);
    }

    #[test]
    fn test_ship_upgraded_event_has_indexed_topics() {
        let env   = make_env();
        let owner = Address::generate(&env);
        emit_ship_upgraded(&env, owner, 99, 5);
        assert_event_published(&env, TOPIC_SHIP, ACTION_UPGRADED);
    }

    #[test]
    fn test_bond_created_event_has_indexed_topics() {
        let env     = make_env();
        let party_a = Address::generate(&env);
        let party_b = Address::generate(&env);
        emit_bond_created(&env, party_a, party_b, 1001);
        assert_event_published(&env, TOPIC_BOND, ACTION_CREATED);
    }

    #[test]
    fn test_bond_accepted_event_has_indexed_topics() {
        let env     = make_env();
        let party_b = Address::generate(&env);
        emit_bond_accepted(&env, 1001, party_b);
        assert_event_published(&env, TOPIC_BOND, ACTION_ACCEPTED);
    }

    #[test]
    fn test_yield_delegated_event_has_indexed_topics() {
        let env       = make_env();
        let delegator = Address::generate(&env);
        emit_yield_delegated(&env, 1001, delegator, 20, 500);
        assert_event_published(&env, TOPIC_BOND, ACTION_DELEGATED);
    }

    #[test]
    fn test_yield_claimed_event_has_indexed_topics() {
        let env         = make_env();
        let beneficiary = Address::generate(&env);
        emit_yield_claimed(&env, 1001, beneficiary, 100);
        assert_event_published(&env, TOPIC_BOND, ACTION_CLAIMED);
    }

    #[test]
    fn test_bond_dissolved_event_has_indexed_topics() {
        let env       = make_env();
        let initiator = Address::generate(&env);
        emit_bond_dissolved(&env, 1001, initiator);
        assert_event_published(&env, TOPIC_BOND, ACTION_DISSOLVED);
    }

    #[test]
    fn test_rate_limit_hit_event_has_indexed_topics() {
        let env    = make_env();
        let caller = Address::generate(&env);
        emit_rate_limit_hit(&env, caller, symbol_short!("NebulaGen"), 6, 5);
        assert_event_published(&env, TOPIC_RATE, ACTION_HIT);
    }

    #[test]
    fn test_all_bond_topics_are_distinct() {
        // All five bond actions must have different ACTION symbols
        let actions = [
            ACTION_CREATED, ACTION_ACCEPTED, ACTION_DELEGATED,
            ACTION_CLAIMED, ACTION_DISSOLVED,
        ];
        for i in 0..actions.len() {
            for j in (i + 1)..actions.len() {
                assert_ne!(
                    actions[i], actions[j],
                    "Bond action symbols at indices {} and {} collide", i, j
                );
            }
        }
    }
}
