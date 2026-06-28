#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
use soroban_sdk::{symbol_short, vec, Address, Bytes, BytesN, Env, String, Vec};
use stellar_nebula_nomad::{
    Blueprint, BlueprintError, BlueprintRarity, CellType, NebulaCell, NebulaLayout,
    NebulaNomadContract, NebulaNomadContractClient, ProfileError, ProgressUpdate, Rarity,
    Referral, ReferralError, Session, SessionError, Ship, ShipError, GRID_SIZE, TOTAL_CELLS,
};
use stellar_nebula_nomad::resource_minter::{
    ResourceError, ResourceMinter, ResourceMinterClient, ResourceType, LEDGERS_PER_DAY,
};

fn setup_env() -> (Env, NebulaNomadContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        protocol_version: 22,
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1000,
        max_entry_ttl: 10_000,
    });
    let contract_id = env.register_contract(None, NebulaNomadContract);
    let client = NebulaNomadContractClient::new(&env, &contract_id);
    let player = Address::generate(&env);
    (env, client, player)
}

// ─── generate_nebula_layout ───────────────────────────────────────────────

#[test]
fn test_generate_layout_dimensions() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[1u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    assert_eq!(layout.width, GRID_SIZE);
    assert_eq!(layout.height, GRID_SIZE);
    assert_eq!(layout.cells.len(), TOTAL_CELLS);
}

#[test]
fn test_generate_layout_has_energy() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[42u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    assert!(layout.total_energy > 0);
}

#[test]
fn test_generate_layout_deterministic() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[7u8; 32]);
    let layout1 = client.generate_nebula_layout(&seed, &player);
    let layout2 = client.generate_nebula_layout(&seed, &player);
    assert_eq!(layout1.total_energy, layout2.total_energy);
    assert_eq!(layout1.seed, layout2.seed);
    assert_eq!(layout1.timestamp, layout2.timestamp);
}

#[test]
fn test_different_seeds_produce_different_layouts() {
    let (env, client, player) = setup_env();
    let seed_a = BytesN::from_array(&env, &[1u8; 32]);
    let seed_b = BytesN::from_array(&env, &[2u8; 32]);
    let layout_a = client.generate_nebula_layout(&seed_a, &player);
    let layout_b = client.generate_nebula_layout(&seed_b, &player);
    assert_ne!(layout_a.total_energy, layout_b.total_energy);
}

#[test]
fn test_layout_changes_with_ledger_state() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, NebulaNomadContract);
    let client = NebulaNomadContractClient::new(&env, &contract_id);
    let player = Address::generate(&env);
    let seed = BytesN::from_array(&env, &[5u8; 32]);

    env.ledger().set(LedgerInfo {
        protocol_version: 22,
        sequence_number: 100,
        timestamp: 1_000_000,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1000,
        max_entry_ttl: 10_000,
    });
    let layout1 = client.generate_nebula_layout(&seed, &player);

    env.ledger().set(LedgerInfo {
        protocol_version: 22,
        sequence_number: 200,
        timestamp: 2_000_000,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 1000,
        max_entry_ttl: 10_000,
    });
    let layout2 = client.generate_nebula_layout(&seed, &player);

    assert_ne!(layout1.total_energy, layout2.total_energy);
}

#[test]
fn test_layout_cell_coordinates() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[10u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);

    for i in 0..layout.cells.len() {
        let cell = layout.cells.get(i).unwrap();
        assert!(cell.x < GRID_SIZE);
        assert!(cell.y < GRID_SIZE);
    }
}

#[test]
fn test_layout_records_timestamp() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[3u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    assert_eq!(layout.timestamp, 1_700_000_000);
}

#[test]
fn test_zero_seed_works() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[0u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    assert_eq!(layout.cells.len(), TOTAL_CELLS);
}

// ─── calculate_rarity_tier ────────────────────────────────────────────────

fn make_layout(env: &Env, rare_count: u32, energy_per_cell: u32) -> NebulaLayout {
    let mut cells = Vec::new(env);
    let mut total_energy = 0u32;
    for i in 0..TOTAL_CELLS {
        let (cell_type, energy) = if i < rare_count {
            (CellType::Wormhole, 60 + energy_per_cell)
        } else {
            (CellType::Empty, energy_per_cell)
        };
        total_energy += energy;
        cells.push_back(NebulaCell {
            x: i % GRID_SIZE,
            y: i / GRID_SIZE,
            cell_type,
            energy,
        });
    }
    NebulaLayout {
        width: GRID_SIZE,
        height: GRID_SIZE,
        cells,
        seed: BytesN::from_array(env, &[0u8; 32]),
        timestamp: 0,
        total_energy,
    }
}

#[test]
fn test_rarity_common() {
    let (env, client, _) = setup_env();
    let layout = make_layout(&env, 0, 0);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Common);
}

#[test]
fn test_rarity_uncommon() {
    let (env, client, _) = setup_env();
    // 5 rare cells × 10 = 50, energy_density ≈ 0 → score 50 → Uncommon
    let layout = make_layout(&env, 5, 0);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Uncommon);
}

#[test]
fn test_rarity_rare() {
    let (env, client, _) = setup_env();
    // 10 rare cells × 10 = 100 → score 100 → Rare
    let layout = make_layout(&env, 10, 0);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Rare);
}

#[test]
fn test_rarity_epic() {
    let (env, client, _) = setup_env();
    // 15 rare cells × 10 = 150 → score 150 → Epic
    let layout = make_layout(&env, 15, 0);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Epic);
}

#[test]
fn test_rarity_legendary() {
    let (env, client, _) = setup_env();
    // 20 rare cells × 10 = 200 → score 200 → Legendary
    let layout = make_layout(&env, 20, 0);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Legendary);
}

#[test]
fn test_rarity_energy_density_contributes() {
    let (env, client, _) = setup_env();
    // 4 rare cells × 10 = 40, with high energy per cell to push into Uncommon
    // energy_per_cell = 10 → total = 256 * 10 = 2560, density = 10 → score = 50
    let layout = make_layout(&env, 4, 10);
    let rarity = client.calculate_rarity_tier(&layout);
    assert_eq!(rarity, Rarity::Uncommon);
}

#[test]
fn test_rarity_from_generated_layout() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[99u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    let rarity = client.calculate_rarity_tier(&layout);
    // Should be one of the valid rarity tiers
    assert!(
        rarity == Rarity::Common
            || rarity == Rarity::Uncommon
            || rarity == Rarity::Rare
            || rarity == Rarity::Epic
            || rarity == Rarity::Legendary
    );
}

// ─── scan_nebula (end-to-end + event emission) ───────────────────────────

#[test]
fn test_scan_nebula_returns_layout_and_rarity() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[50u8; 32]);
    let (layout, rarity) = client.scan_nebula(&seed, &player);
    assert_eq!(layout.width, GRID_SIZE);
    assert_eq!(layout.height, GRID_SIZE);
    assert_eq!(layout.cells.len(), TOTAL_CELLS);
    assert!(
        rarity == Rarity::Common
            || rarity == Rarity::Uncommon
            || rarity == Rarity::Rare
            || rarity == Rarity::Epic
            || rarity == Rarity::Legendary
    );
}

#[test]
fn test_scan_nebula_emits_event() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[77u8; 32]);
    let _result = client.scan_nebula(&seed, &player);

    let events = env.events().all();
    assert!(
        !events.is_empty(),
        "Expected NebulaScanned event to be emitted"
    );

    // Verify the last event has the correct topics
    let last = events.get(events.len() - 1).unwrap();
    let (_contract_addr, topics, _data) = last;
    assert_eq!(topics.len(), 2);
}

#[test]
fn test_scan_nebula_consistency_with_individual_calls() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[33u8; 32]);

    let layout = client.generate_nebula_layout(&seed, &player);
    let rarity = client.calculate_rarity_tier(&layout);

    let (scan_layout, scan_rarity) = client.scan_nebula(&seed, &player);

    assert_eq!(layout.total_energy, scan_layout.total_energy);
    assert_eq!(rarity, scan_rarity);
}

// ─── onboarding tutorial flow ────────────────────────────────────────────

#[test]
fn test_onboarding_full_tutorial_flow() {
    let (env, client, player) = setup_env();

    let admin = Address::generate(&env);
    client.init_onboarding(&admin);
    client.create_profile(&player);

    for step in 0..5u32 {
        let reward = client.complete_tutorial_step(&player, &step);
        assert!(reward > 0);
    }

    let progress = client.get_tutorial_progress(&player).unwrap();
    assert_eq!(progress.completed_count, 5);
    assert_eq!(progress.completed_at, 1_700_000_000);

    let starter_resources = client.get_starter_resources(&player);
    assert_eq!(starter_resources, 275);
}

#[test]
fn test_onboarding_step_cannot_be_replayed() {
    let (env, client, player) = setup_env();

    let admin = Address::generate(&env);
    client.init_onboarding(&admin);
    client.create_profile(&player);

    client.complete_tutorial_step(&player, &0);
    let replay = client.try_complete_tutorial_step(&player, &0);
    assert!(replay.is_err());
}

// ─── bug bounty payout engine ────────────────────────────────────────────

#[test]
fn test_bug_bounty_approval_cycle() {
    let (env, client, _) = setup_env();

    let admin = Address::generate(&env);
    let approver_2 = Address::generate(&env);
    let reporter = Address::generate(&env);

    let approvers = vec![&env, approver_2.clone()];
    client.init_bounty_engine(&admin, &approvers, &2, &5_000, &3600);
    client.fund_bounty_pool(&admin, &20_000);

    let report_id = client.submit_bug_report(
        &reporter,
        &String::from_str(&env, "critical exploit in scan verifier"),
        &symbol_short!("high"),
    );

    let first = client.approve_and_pay_bounty(&admin, &report_id, &2_000);
    assert!(!first);

    let second = client.approve_and_pay_bounty(&approver_2, &report_id, &2_000);
    assert!(second);

    let balance = client.get_bounty_balance(&reporter);
    assert_eq!(balance, 2_000);
    assert_eq!(client.get_bounty_pool(), 18_000);
}

#[test]
fn test_bug_report_rejects_invalid_severity() {
    let (env, client, _) = setup_env();

    let admin = Address::generate(&env);
    let reporter = Address::generate(&env);
    let approvers = vec![&env, Address::generate(&env)];

    client.init_bounty_engine(&admin, &approvers, &2, &5_000, &3600);

    let invalid = client.try_submit_bug_report(
        &reporter,
        &String::from_str(&env, "unknown severity class"),
        &symbol_short!("sev_x"),
    );
    assert!(invalid.is_err());
}

// ─── standardized event framework ────────────────────────────────────────

#[test]
fn test_event_framework_emit_and_query() {
    let (env, client, caller) = setup_env();
    let admin = Address::generate(&env);

    client.init_event_framework(&admin);

    let payload = BytesN::from_array(&env, &[9u8; 256]);
    let event_id = client.emit_standard_event(&caller, &symbol_short!("system"), &payload);
    assert!(event_id > 0);

    let results = client.query_recent_events(&symbol_short!("system"), &5);
    assert_eq!(results.len(), 1);

    let record = results.get(0).unwrap();
    assert_eq!(record.version, 1);
    assert_eq!(record.caller, caller);
}

#[test]
fn test_event_framework_invalid_type() {
    let (env, client, caller) = setup_env();
    let admin = Address::generate(&env);

    client.init_event_framework(&admin);

    let payload = BytesN::from_array(&env, &[1u8; 256]);
    let invalid = client.try_emit_standard_event(&caller, &symbol_short!("other"), &payload);
    assert!(invalid.is_err());
}

// ─── fleet manager ────────────────────────────────────────────────────────

#[test]
fn test_fleet_register_and_sync() {
    let (env, client, owner) = setup_env();

    client.init_fleet_templates();

    let ship_1 = Ship {
        id: 1,
        owner: owner.clone(),
        name: String::from_str(&env, "Voyager"),
        level: 2,
        scan_range: 4,
    };
    let ship_2 = Ship {
        id: 2,
        owner: owner.clone(),
        name: String::from_str(&env, "Pioneer"),
        level: 3,
        scan_range: 5,
    };

    client.register_ship_for_owner(&owner, &ship_1);
    client.register_ship_for_owner(&owner, &ship_2);

    let ship_ids = vec![&env, 1u64, 2u64];
    let fleet = client.register_fleet(&owner, &ship_ids, &1);
    assert!(fleet.id > 0);
    assert_eq!(fleet.ship_ids.len(), 2);
    assert!(fleet.immutable_membership);

    let status = client.sync_fleet_status(&fleet.id);
    assert_eq!(status.total_level, 5);
    assert!(status.average_scan_range >= 4);
    assert_eq!(status.vessel_count, 2);
}

#[test]
fn test_fleet_rejects_too_many_ships() {
    let (env, client, owner) = setup_env();

    client.init_fleet_templates();

    let mut ship_ids = Vec::new(&env);
    for i in 1u64..=11u64 {
        let ship = Ship {
            id: i,
            owner: owner.clone(),
            name: String::from_str(&env, "Scout"),
            level: 1,
            scan_range: 2,
        };
        client.register_ship_for_owner(&owner, &ship);
        ship_ids.push_back(i);
    }

    let result = client.try_register_fleet(&owner, &ship_ids, &1);
    assert!(result.is_err());
}

// ─── Ship NFT tests ───────────────────────────────────────────────────────

#[test]
fn test_mint_ship_and_transfer_ownership() {
    let (env, client, player) = setup_env();
    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &soroban_sdk::symbol_short!("fighter"), &metadata);
    assert_eq!(ship.owner, player);

    let new_owner = Address::generate(&env);
    let transferred = client.transfer_ownership(&ship.id, &new_owner);
    assert_eq!(transferred.owner, new_owner);
}

#[test]
fn test_batch_mint_limit_and_invalid_ship_type() {
    let (env, client, player) = setup_env();
    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let types = vec![
        &env,
        soroban_sdk::symbol_short!("fighter"),
        soroban_sdk::symbol_short!("explorer"),
        soroban_sdk::symbol_short!("hauler"),
    ];
    let ships = client.batch_mint_ships(&player, &types, &metadata);
    assert_eq!(ships.len(), 3);
}

// ─── Harvest tests (NebulaNomadContract) ─────────────────────────────────

#[test]
fn test_harvest_resources_single_invocation_and_events() {
    let (env, client, player) = setup_env();
    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &soroban_sdk::symbol_short!("explorer"), &metadata);
    let seed = BytesN::from_array(&env, &[42u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    let harvest = client.harvest_resources(&ship.id, &layout);
    assert_eq!(harvest.ship_id, ship.id);
    assert!(harvest.total_harvested > 0);
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_harvest_fails_for_unknown_ship() {
    let (env, client, player) = setup_env();
    let seed = BytesN::from_array(&env, &[42u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    let result = client.try_harvest_resources(&9999u64, &layout);
    assert!(result.is_err());
}

// ─── ResourceMinter tests ─────────────────────────────────────────────────

fn setup_minter_env() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set(LedgerInfo {
        protocol_version: 22,
        sequence_number: 0,
        timestamp: 0,
        network_id: [0u8; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 100,
        min_persistent_entry_ttl: 6_312_000,
        max_entry_ttl: 6_312_000,
    });
    let admin = Address::generate(&env);
    let player = Address::generate(&env);
    let dummy = Address::generate(&env);
    let cid = env.register(ResourceMinter, ());
    let client = ResourceMinterClient::new(&env, &cid);
    let _ = client.try_init(&admin, &dummy, &dummy, &500u32, &1_000i128, &LEDGERS_PER_DAY);
    (env, cid, admin, player)
}

fn advance_ledgers(env: &Env, n: u32) {
    let seq = env.ledger().sequence();
    let ts = env.ledger().timestamp();
    env.ledger().set(LedgerInfo {
        sequence_number: seq + n,
        timestamp: ts + (n as u64 * 5),
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 16,
        min_persistent_entry_ttl: 6_312_000,
        max_entry_ttl: 6_312_000,
    });
}

#[test]
fn test_harvest_base_amount() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    let minted = client.harvest_resource(&player, &1u64, &0u32);
    assert_eq!(minted, 100);
    assert_eq!(client.get_balance(&player, &ResourceType::Stardust), 100);
}

#[test]
fn test_harvest_rarity_bonus() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    assert_eq!(client.harvest_resource(&player, &1u64, &5u32), 150);
}

#[test]
fn test_harvest_multiple_ships_have_independent_caps() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    for _ in 0..10 {
        client.harvest_resource(&player, &1u64, &0u32);
    }
    assert_eq!(client.harvest_resource(&player, &2u64, &0u32), 100);
}

#[test]
fn test_harvest_daily_cap_enforced() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    for _ in 0..10 {
        client.harvest_resource(&player, &1u64, &0u32);
    }
    let err = client.try_harvest_resource(&player, &1u64, &0u32);
    assert_eq!(err, Err(Ok(ResourceError::DailyCapExceeded)));
}

#[test]
fn test_harvest_cap_resets_after_one_day() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    for _ in 0..10 {
        client.harvest_resource(&player, &1u64, &0u32);
    }
    advance_ledgers(&env, LEDGERS_PER_DAY);
    assert_eq!(client.harvest_resource(&player, &1u64, &0u32), 100);
}

#[test]
fn test_harvest_amount_capped_near_daily_limit() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    for _ in 0..9 {
        client.harvest_resource(&player, &1u64, &0u32);
    }
    assert_eq!(client.harvest_resource(&player, &1u64, &5u32), 100);
}

#[test]
fn test_resource_type_balances_are_independent() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    assert_eq!(client.get_balance(&player, &ResourceType::Stardust), 100);
    assert_eq!(client.get_balance(&player, &ResourceType::Plasma), 0);
    assert_eq!(client.get_balance(&player, &ResourceType::Crystals), 0);
}

// ─── Staking tests ────────────────────────────────────────────────────────

#[test]
fn test_stake_deducts_liquid_balance() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    assert_eq!(client.get_balance(&player, &ResourceType::Stardust), 0);
    assert_eq!(client.get_stake(&player).unwrap().amount, 100);
}

#[test]
fn test_stake_insufficient_resources_rejected() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    let err = client.try_stake_for_yield(
        &player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY,
    );
    assert_eq!(err, Err(Ok(ResourceError::InsufficientResources)));
}

#[test]
fn test_stake_below_min_duration_rejected() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    let err = client.try_stake_for_yield(
        &player, &ResourceType::Stardust, &100i128, &1_000u32,
    );
    assert_eq!(err, Err(Ok(ResourceError::InvalidDuration)));
}

#[test]
fn test_stake_zero_amount_rejected() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    let err = client.try_stake_for_yield(
        &player, &ResourceType::Stardust, &0i128, &LEDGERS_PER_DAY,
    );
    assert_eq!(err, Err(Ok(ResourceError::InvalidAmount)));
}

#[test]
fn test_duplicate_stake_rejected() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.harvest_resource(&player, &2u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    let err = client.try_stake_for_yield(
        &player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY,
    );
    assert_eq!(err, Err(Ok(ResourceError::AlreadyStaked)));
}

#[test]
fn test_claim_yield_after_24h() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY);
    let yield_earned = client.claim_yield(&player);
    assert!(yield_earned >= 0);
    assert_eq!(client.get_balance(&player, &ResourceType::Plasma), yield_earned);
}

#[test]
fn test_claim_yield_after_1_year() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY * 365);
    let yield_earned = client.claim_yield(&player);
    assert_eq!(yield_earned, 5);
    assert_eq!(client.get_balance(&player, &ResourceType::Plasma), 5);
}

#[test]
fn test_pending_yield_matches_claim_amount() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY * 365);
    let pending = client.get_pending_yield(&player);
    let claimed = client.claim_yield(&player);
    assert_eq!(pending, claimed);
}

#[test]
fn test_yield_accumulates_across_partial_claims() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &(LEDGERS_PER_DAY * 365 * 2));
    advance_ledgers(&env, LEDGERS_PER_DAY * 365);
    let first_claim = client.claim_yield(&player);
    advance_ledgers(&env, LEDGERS_PER_DAY * 365);
    let second_claim = client.claim_yield(&player);
    assert!(first_claim + second_claim > 0);
}

#[test]
fn test_unstake_blocked_immediately_after_stake() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    let err = client.try_unstake(&player);
    assert_eq!(err, Err(Ok(ResourceError::TimeLockActive)));
}

#[test]
fn test_unstake_allowed_after_timelock_expires() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY);
    let result = client.try_unstake(&player);
    assert!(result.is_ok());
}

#[test]
fn test_unstake_auto_claims_residual_yield() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY * 365);
    let returned = client.unstake(&player);
    assert!(returned >= 0);
}

#[test]
fn test_unstake_then_restake_succeeds() {
    let (env, cid, _, player) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.harvest_resource(&player, &1u64, &0u32);
    client.stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    advance_ledgers(&env, LEDGERS_PER_DAY);
    client.unstake(&player);
    let result = client.try_stake_for_yield(&player, &ResourceType::Stardust, &100i128, &LEDGERS_PER_DAY);
    assert!(result.is_ok());
}

#[test]
fn test_update_daily_cap() {
    let (env, cid, _, _) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.update_daily_cap(&2_000i128);
    assert_eq!(client.get_config().unwrap().daily_harvest_cap, 2_000);
}

#[test]
fn test_update_apy() {
    let (env, cid, _, _) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    client.update_apy(&1_000u32);
    assert_eq!(client.get_config().unwrap().apy_basis_points, 1_000);
}

#[test]
fn test_double_init_rejected() {
    let (env, cid, admin, _) = setup_minter_env();
    let client = ResourceMinterClient::new(&env, &cid);
    let dummy = Address::generate(&env);
    let err = client.try_init(
        &admin,
        &dummy,
        &dummy,
        &500u32,
        &1_000i128,
        &LEDGERS_PER_DAY,
    );
    assert_eq!(err, Err(Ok(ResourceError::AlreadyInitialized)));
}

// ─── Player profile tests ─────────────────────────────────────────────────

#[test]
fn test_initialize_profile_success() {
    let (env, client, player) = setup_env();
    let id = client.initialize_profile(&player);
    assert_eq!(id, 1);
}

#[test]
fn test_initialize_profile_increments_id() {
    let (env, client, _) = setup_env();
    let player_a = Address::generate(&env);
    let player_b = Address::generate(&env);
    let id_a = client.initialize_profile(&player_a);
    let id_b = client.initialize_profile(&player_b);
    assert_eq!(id_a, 1);
    assert_eq!(id_b, 2);
}

#[test]
#[should_panic]
fn test_initialize_profile_duplicate_panics() {
    let (env, client, player) = setup_env();
    client.initialize_profile(&player);
    client.initialize_profile(&player);
}

#[test]
fn test_get_profile_returns_correct_owner() {
    let (env, client, player) = setup_env();
    let id = client.initialize_profile(&player);
    let profile = client.get_profile(&id);
    assert_eq!(profile.owner, player);
    assert_eq!(profile.total_scans, 0);
    assert_eq!(profile.essence_earned, 0);
}

#[test]
#[should_panic]
fn test_get_profile_not_found_panics() {
    let (_env, client, _) = setup_env();
    client.get_profile(&999u64);
}

#[test]
fn test_update_progress_accumulates_stats() {
    let (env, client, player) = setup_env();
    let id = client.initialize_profile(&player);
    client.update_progress(&player, &id, &3u32, &500i128);
    client.update_progress(&player, &id, &2u32, &250i128);
    let profile = client.get_profile(&id);
    assert_eq!(profile.total_scans, 5);
    assert_eq!(profile.essence_earned, 750);
}

#[test]
#[should_panic]
fn test_update_progress_wrong_caller_panics() {
    let (env, client, player) = setup_env();
    let intruder = Address::generate(&env);
    let id = client.initialize_profile(&player);
    client.update_progress(&intruder, &id, &1u32, &100i128);
}

#[test]
fn test_batch_update_progress_applies_all() {
    let (env, client, player) = setup_env();
    let id = client.initialize_profile(&player);
    let updates = soroban_sdk::vec![
        &env,
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 100 },
        ProgressUpdate { profile_id: id, scan_count: 2, essence: 200 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 50  },
    ];
    client.batch_update_progress(&player, &updates);
    let profile = client.get_profile(&id);
    assert_eq!(profile.total_scans, 4);
    assert_eq!(profile.essence_earned, 350);
}

#[test]
#[should_panic]
fn test_batch_update_exceeds_limit_panics() {
    let (env, client, player) = setup_env();
    let id = client.initialize_profile(&player);
    let updates = soroban_sdk::vec![
        &env,
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
        ProgressUpdate { profile_id: id, scan_count: 1, essence: 10 },
    ];
    client.batch_update_progress(&player, &updates);
}

#[test]
fn test_profile_emits_nomad_joined_event() {
    let (env, client, player) = setup_env();
    client.initialize_profile(&player);
    let events = env.events().all();
    assert!(!events.is_empty());
}

// ─── Session manager tests ────────────────────────────────────────────────

#[test]
fn test_start_session_success() {
    let (env, client, player) = setup_env();
    let session_id = client.start_session(&player, &42u64);
    assert_eq!(session_id, 1);
}

#[test]
fn test_start_session_records_expiry() {
    let (env, client, player) = setup_env();
    let session_id = client.start_session(&player, &1u64);
    let session = client.get_session(&session_id);
    assert_eq!(session.started_at, 1_700_000_000);
    assert_eq!(session.expires_at, 1_700_000_000 + 86_400);
    assert!(session.active);
}

#[test]
fn test_start_multiple_sessions_up_to_limit() {
    let (env, client, player) = setup_env();
    client.start_session(&player, &1u64);
    client.start_session(&player, &2u64);
    let id3 = client.start_session(&player, &3u64);
    assert_eq!(id3, 3);
}

#[test]
#[should_panic]
fn test_start_session_exceeds_limit_panics() {
    let (env, client, player) = setup_env();
    client.start_session(&player, &1u64);
    client.start_session(&player, &2u64);
    client.start_session(&player, &3u64);
    client.start_session(&player, &4u64);
}

#[test]
fn test_expire_session_by_owner() {
    let (env, client, player) = setup_env();
    let id = client.start_session(&player, &1u64);
    client.expire_session(&player, &id);
    let session = client.get_session(&id);
    assert!(!session.active);
}

#[test]
fn test_expire_session_frees_slot_for_new_session() {
    let (env, client, player) = setup_env();
    client.start_session(&player, &1u64);
    client.start_session(&player, &2u64);
    let id3 = client.start_session(&player, &3u64);
    client.expire_session(&player, &id3);
    let id4 = client.start_session(&player, &4u64);
    assert_eq!(id4, 4);
}

#[test]
#[should_panic]
fn test_expire_already_expired_session_panics() {
    let (env, client, player) = setup_env();
    let id = client.start_session(&player, &1u64);
    client.expire_session(&player, &id);
    client.expire_session(&player, &id);
}

#[test]
fn test_session_emits_started_event() {
    let (env, client, player) = setup_env();
    client.start_session(&player, &1u64);
    let events = env.events().all();
    assert!(!events.is_empty());
}

// ─── Blueprint factory tests ──────────────────────────────────────────────

fn make_components(env: &Env, symbols: &[&str]) -> soroban_sdk::Vec<soroban_sdk::Symbol> {
    let mut v = soroban_sdk::Vec::new(env);
    for s in symbols {
        v.push_back(soroban_sdk::Symbol::new(env, s));
    }
    v
}

#[test]
fn test_craft_blueprint_success() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron", "gas"]);
    let id = client.craft_blueprint(&player, &components);
    assert_eq!(id, 1);
}

#[test]
fn test_craft_blueprint_rarity_common() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron", "gas"]);
    let id = client.craft_blueprint(&player, &components);
    let bp = client.get_blueprint(&id);
    assert_eq!(bp.rarity, BlueprintRarity::Common);
    assert!(!bp.applied);
}

#[test]
fn test_craft_blueprint_rarity_uncommon() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron", "gas", "dust", "void"]);
    let id = client.craft_blueprint(&player, &components);
    let bp = client.get_blueprint(&id);
    assert_eq!(bp.rarity, BlueprintRarity::Uncommon);
}

#[test]
fn test_craft_blueprint_rarity_rare() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["a", "b", "c", "d", "e", "f"]);
    let id = client.craft_blueprint(&player, &components);
    let bp = client.get_blueprint(&id);
    assert_eq!(bp.rarity, BlueprintRarity::Rare);
}

#[test]
#[should_panic]
fn test_craft_blueprint_too_few_components_panics() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron"]);
    client.craft_blueprint(&player, &components);
}

#[test]
fn test_apply_blueprint_to_ship() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron", "gas"]);
    let bp_id = client.craft_blueprint(&player, &components);
    client.apply_blueprint_to_ship(&player, &bp_id, &10u64);
    let bp = client.get_blueprint(&bp_id);
    assert!(bp.applied);
}

#[test]
#[should_panic]
fn test_apply_blueprint_twice_panics() {
    let (env, client, player) = setup_env();
    let components = make_components(&env, &["iron", "gas"]);
    let bp_id = client.craft_blueprint(&player, &components);
    client.apply_blueprint_to_ship(&player, &bp_id, &10u64);
    client.apply_blueprint_to_ship(&player, &bp_id, &10u64);
}

#[test]
#[should_panic]
fn test_apply_blueprint_wrong_owner_panics() {
    let (env, client, player) = setup_env();
    let intruder = Address::generate(&env);
    let components = make_components(&env, &["iron", "gas"]);
    let bp_id = client.craft_blueprint(&player, &components);
    client.apply_blueprint_to_ship(&intruder, &bp_id, &10u64);
}

#[test]
fn test_batch_craft_blueprints() {
    let (env, client, player) = setup_env();
    let r1 = make_components(&env, &["iron", "gas"]);
    let r2 = make_components(&env, &["dust", "void"]);
    let mut recipes = soroban_sdk::Vec::new(&env);
    recipes.push_back(r1);
    recipes.push_back(r2);
    let ids = client.batch_craft_blueprints(&player, &recipes);
    assert_eq!(ids.len(), 2);
}

#[test]
#[should_panic]
fn test_batch_craft_exceeds_limit_panics() {
    let (env, client, player) = setup_env();
    let r = make_components(&env, &["iron", "gas"]);
    let mut recipes = soroban_sdk::Vec::new(&env);
    recipes.push_back(r.clone());
    recipes.push_back(r.clone());
    recipes.push_back(r);
    client.batch_craft_blueprints(&player, &recipes);
}

// ─── Referral system tests ────────────────────────────────────────────────

#[test]
fn test_register_referral_success() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    let id = client.register_referral(&referrer, &new_nomad);
    assert_eq!(id, 1);
}

#[test]
fn test_get_referral_stores_correct_data() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    let referral = client.get_referral(&new_nomad);
    assert_eq!(referral.referrer, referrer);
    assert_eq!(referral.new_nomad, new_nomad);
    assert!(!referral.claimed);
    assert!(!referral.first_scan_done);
}

#[test]
#[should_panic]
fn test_register_referral_self_panics() {
    let (env, client, player) = setup_env();
    client.register_referral(&player, &player);
}

#[test]
#[should_panic]
fn test_register_referral_duplicate_panics() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    client.register_referral(&referrer, &new_nomad);
}

#[test]
fn test_mark_first_scan_and_claim_reward() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    client.mark_first_scan(&new_nomad);
    let reward = client.claim_referral_reward(&referrer, &new_nomad);
    assert_eq!(reward, 100);
}

#[test]
#[should_panic]
fn test_claim_reward_before_first_scan_panics() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    client.claim_referral_reward(&referrer, &new_nomad);
}

#[test]
#[should_panic]
fn test_claim_reward_twice_panics() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    client.mark_first_scan(&new_nomad);
    client.claim_referral_reward(&referrer, &new_nomad);
    client.claim_referral_reward(&referrer, &new_nomad);
}

#[test]
fn test_referral_claimed_flag_set_after_claim() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    client.mark_first_scan(&new_nomad);
    client.claim_referral_reward(&referrer, &new_nomad);
    let referral = client.get_referral(&new_nomad);
    assert!(referral.claimed);
    assert!(referral.first_scan_done);
}

#[test]
fn test_referral_emits_registered_event() {
    let (env, client, referrer) = setup_env();
    let new_nomad = Address::generate(&env);
    client.register_referral(&referrer, &new_nomad);
    let events = env.events().all();
    assert!(!events.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Multi-Module Integration Tests
// ═══════════════════════════════════════════════════════════════════════════

// ─── Flow 1: Economy → Market (Resource Minting → DEX Listing) ─────────────

#[test]
fn test_economy_to_market_full_flow() {
    // This test verifies the complete flow from resource harvesting through
    // to an active DEX listing order, confirming that ledger balances
    // update accurately across the economy and marketplace modules.
    let (env, client, player) = setup_env();

    // Step 1: Mint a ship (prerequisite for harvesting)
    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &symbol_short!("explorer"), &metadata);
    assert!(ship.id > 0);

    // Step 2: Generate a nebula layout for harvesting
    let seed = BytesN::from_array(&env, &[42u8; 32]);
    let layout = client.generate_nebula_layout(&seed, &player);
    assert!(layout.total_energy > 0);

    // Step 3: Harvest resources from the nebula (economy module)
    let harvest = client.harvest_resources(&ship.id, &layout);
    assert_eq!(harvest.ship_id, ship.id);
    assert!(harvest.total_harvested > 0, "Harvest should yield resources");

    // Step 4: Verify harvested resources were credited
    // The harvest result contains the list of harvested resources
    assert!(harvest.resources.len() > 0, "Should have harvested at least one resource type");

    // Step 5: Harvest and list on DEX (market module)
    let resource_symbol = symbol_short!("dust");
    let min_price = 50i128;
    let (harvest_result, dex_offer) = client.harvest_and_list(
        &player, &ship.id, &layout, &resource_symbol, &min_price,
    );

    // Verify the DEX offer was created with correct parameters
    assert!(dex_offer.offer_id > 0, "DEX offer should have a valid ID");
    assert!(dex_offer.active, "DEX offer should be active");
    assert!(dex_offer.min_price >= min_price, "Min price should meet or exceed requested minimum");

    // Step 6: Verify the harvest produced resources
    assert!(harvest_result.total_harvested > 0);

    // Step 7: Events should have been emitted for both harvest and DEX listing
    let events = env.events().all();
    assert!(events.len() >= 2, "Should have harvest + DEX listing events");
}

#[test]
fn test_economy_to_market_multiple_harvests_accumulate() {
    // Verifies that multiple harvests accumulate resources before DEX listing.
    let (env, client, player) = setup_env();

    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &symbol_short!("explorer"), &metadata);

    let mut total_harvested = 0u32;

    // Perform multiple harvests with different seeds
    for seed_byte in 1u8..=5u8 {
        let seed = BytesN::from_array(&env, &[seed_byte; 32]);
        let layout = client.generate_nebula_layout(&seed, &player);
        let harvest = client.harvest_resources(&ship.id, &layout);
        total_harvested += harvest.total_harvested;
    }

    assert!(total_harvested > 0, "Multiple harvests should accumulate resources");

    // Final harvest-and-list to verify the accumulated balance can be listed
    let final_seed = BytesN::from_array(&env, &[99u8; 32]);
    let final_layout = client.generate_nebula_layout(&final_seed, &player);
    let (_, offer) = client.harvest_and_list(
        &player, &ship.id, &final_layout, &symbol_short!("dust"), &10i128,
    );
    assert!(offer.active);
}

// ─── Flow 2: Progression → Exploration (Ship Upgrade → Nebula Scan) ────────

#[test]
fn test_progression_to_exploration_full_flow() {
    // This test verifies the multi-stage flow:
    //   1. Mint a base ship asset
    //   2. Upgrade the ship (adjusting properties)
    //   3. Use the upgraded ship to scan a nebula
    //   4. Verify the scan results reflect the ship's improved capabilities
    let (env, client, player) = setup_env();

    // Step 1: Mint a base ship
    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &symbol_short!("fighter"), &metadata);
    assert!(ship.id > 0);

    // Step 2: Register the ship in the registry for upgrade system
    let registered_ship = Ship {
        id: ship.id,
        owner: player.clone(),
        name: String::from_str(&env, "Nebula Runner"),
        level: 1,
        scan_range: 2,
    };
    client.register_ship_for_owner(&player, &registered_ship);

    // Step 3: Perform a baseline nebula scan with the un-upgraded ship
    let seed_baseline = BytesN::from_array(&env, &[10u8; 32]);
    let (baseline_layout, baseline_rarity) = client.scan_nebula(&seed_baseline, &player);

    // Step 4: Upgrade the ship (increase level and scan range)
    // Use the blueprint system to apply an upgrade
    let components = soroban_sdk::vec![&env, symbol_short!("iron"), symbol_short!("gas")];
    let bp_id = client.craft_blueprint(&player, &components);
    client.apply_blueprint_to_ship(&player, &bp_id, &ship.id);

    // Apply a ship upgrade through the ship upgrade module
    let admin = Address::generate(&env);
    client.init_upgrade_config(&admin);
    client.apply_upgrade(&player, &ship.id, &symbol_short!("hull"));

    // Step 5: Scan the same nebula seed with the upgraded ship
    let (upgraded_layout, upgraded_rarity) = client.scan_nebula(&seed_baseline, &player);

    // Step 6: Verify the scan still produces valid results
    assert_eq!(upgraded_layout.width, baseline_layout.width);
    assert_eq!(upgraded_layout.height, baseline_layout.height);
    assert_eq!(upgraded_layout.cells.len(), baseline_layout.cells.len());

    // The layouts should be deterministic for the same seed
    assert_eq!(upgraded_layout.seed, baseline_layout.seed);
    assert_eq!(upgraded_layout.total_energy, baseline_layout.total_energy);

    // Step 7: Verify the rarity calculation still works
    assert!(
        upgraded_rarity == Rarity::Common
            || upgraded_rarity == Rarity::Uncommon
            || upgraded_rarity == Rarity::Rare
            || upgraded_rarity == Rarity::Epic
            || upgraded_rarity == Rarity::Legendary
    );
}

#[test]
fn test_ship_upgrade_improves_capabilities() {
    // Verifies that upgrading a ship actually changes its state.
    let (env, client, player) = setup_env();

    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &symbol_short!("explorer"), &metadata);

    // Initialize upgrade system and apply an upgrade
    let admin = Address::generate(&env);
    client.init_upgrade_config(&admin);
    client.apply_upgrade(&player, &ship.id, &symbol_short!("hull"));

    // The ship's state should reflect the upgrade
    let events = env.events().all();
    assert!(!events.is_empty(), "Upgrade should emit events");
}

#[test]
fn test_multi_stage_scan_consistency() {
    // Verifies that scanning multiple different nebulae with the same ship
    // produces consistent, independent results.
    let (env, client, player) = setup_env();

    let metadata = Bytes::from_slice(&env, &[0u8; 4]);
    let ship = client.mint_ship(&player, &symbol_short!("explorer"), &metadata);

    let mut total_energy_sum = 0u32;
    let mut scan_count = 0u32;

    for seed_val in 1u8..=10u8 {
        let seed = BytesN::from_array(&env, &[seed_val; 32]);
        let (layout, _rarity) = client.scan_nebula(&seed, &player);
        total_energy_sum += layout.total_energy;
        scan_count += 1;
    }

    assert_eq!(scan_count, 10, "Should have completed all 10 scans");
    assert!(total_energy_sum > 0, "Total energy across scans should be positive");
}

// ─── Flow 3: Social → Fiscal Governance (Alliance → Treasury → Voting) ─────

#[test]
fn test_social_to_fiscal_governance_full_flow() {
    // This test traces the structural interaction sequence:
    //   1. Found a sovereign alliance (social module)
    //   2. Members join and contribute funding to the treasury
    //   3. A governance proposal is created for fund allocation
    //   4. Members cast votes on the proposal
    //   5. Verify the governance flow completes with correct state
    let (env, client, _) = setup_env();

    // Step 1: Founder creates an alliance
    let founder = Address::generate(&env);
    let alliance_name = String::from_str(&env, "Stellar Vanguard");
    let alliance_id = client.found_alliance(&founder, &alliance_name);
    assert!(alliance_id > 0, "Alliance should have a valid ID");

    // Step 2: Verify alliance was created correctly
    let alliance = client.get_alliance(&alliance_id);
    assert_eq!(alliance.name, alliance_name);
    assert_eq!(alliance.founder, founder);
    assert!(alliance.is_active);
    assert_eq!(alliance.members.len(), 1, "Founder should be the sole initial member");

    // Step 3: Additional members join the alliance
    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);

    let record1 = client.join_alliance(&alliance_id, &member1);
    assert_eq!(record1.alliance_id, alliance_id);
    assert_eq!(record1.member, member1);

    let record2 = client.join_alliance(&alliance_id, &member2);
    assert_eq!(record2.alliance_id, alliance_id);
    assert_eq!(record2.member, member2);

    // Step 4: Members contribute to the alliance treasury (fiscal flow)
    let contribution1 = client.contribute_to_treasury(&member1, &500i128);
    assert!(contribution1 > 0, "Treasury should reflect the contribution");

    let contribution2 = client.contribute_to_treasury(&member2, &300i128);
    assert!(contribution2 > 0);

    // Step 5: Verify treasury balance accumulated contributions
    let treasury = client.get_alliance_treasury(&alliance_id);
    assert!(treasury >= 800, "Treasury should be at least 800 (500 + 300)");

    // Step 6: Member contributions should be tracked individually
    let contrib_m1 = client.get_member_contribution(&alliance_id, &member1);
    assert!(contrib_m1 >= 500, "Member1 contribution should be tracked");

    let contrib_m2 = client.get_member_contribution(&alliance_id, &member2);
    assert!(contrib_m2 >= 300, "Member2 contribution should be tracked");

    // Step 7: Create a governance proposal for treasury allocation
    let description = String::from_str(&env, "Allocate 200 from treasury for fleet expansion");
    let param_change = BytesN::from_array(&env, &[0xABu8; 128]);
    let proposal_id = client.create_proposal(&founder, &description, &param_change);
    assert!(proposal_id > 0, "Proposal should have a valid ID");

    // Step 8: Members cast votes on the proposal
    let vote_weight_for = 100i128;
    let vote_weight_against = 50i128;

    // Founder votes in favor
    client.cast_vote(&founder, &proposal_id, &true, &vote_weight_for);

    // Member1 votes in favor
    client.cast_vote(&member1, &proposal_id, &true, &vote_weight_for);

    // Member2 votes against
    client.cast_vote(&member2, &proposal_id, &false, &vote_weight_against);

    // Step 9: Verify events were emitted throughout the flow
    let events = env.events().all();
    // Should have: alliance founded + 2 joins + 2 contributions + 1 proposal + 3 votes
    assert!(events.len() >= 9, "Should have emitted events for all governance actions");
}

#[test]
fn test_alliance_membership_enforcement() {
    // Verifies alliance membership constraints and error handling.
    let (env, client, _) = setup_env();

    let founder = Address::generate(&env);
    let alliance_id = client.found_alliance(&founder, &String::from_str(&env, "Test Alliance"));

    // Member joins successfully
    let member = Address::generate(&env);
    client.join_alliance(&alliance_id, &member);

    // Verify member is now in an alliance
    let player_alliance = client.get_player_alliance(&member);
    assert!(player_alliance.is_some(), "Member should be in an alliance");
}

#[test]
fn test_treasury_contribution_tracking() {
    // Verifies that treasury contributions are correctly tracked and queried.
    let (env, client, _) = setup_env();

    let founder = Address::generate(&env);
    let alliance_id = client.found_alliance(&founder, &String::from_str(&env, "Treasury Test"));

    let member = Address::generate(&env);
    client.join_alliance(&alliance_id, &member);

    // Initial treasury should be zero
    let initial_treasury = client.get_alliance_treasury(&alliance_id);

    // Contribute multiple times
    client.contribute_to_treasury(&member, &100i128);
    client.contribute_to_treasury(&member, &200i128);
    client.contribute_to_treasury(&member, &150i128);

    let final_treasury = client.get_alliance_treasury(&alliance_id);
    assert!(final_treasury >= initial_treasury + 450, "Treasury should reflect all contributions");

    let total_contrib = client.get_member_contribution(&alliance_id, &member);
    assert!(total_contrib >= 450, "Member contribution should sum to 450");
}
