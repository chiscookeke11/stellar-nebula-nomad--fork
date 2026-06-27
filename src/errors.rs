// ============================================================
// errors.rs — Fix #176: Descriptive context in error variants
// Branch: refactor/error-messages
// ============================================================
//
// Changes:
//   • Add context fields (IDs, amounts, bounds) to every error
//     variant where that information is meaningful for debugging.
//   • All modules that used #[contracterror] now reference this
//     central file, or carry their own context-enriched variants.
//
// Rationale: improves DX for SDK callers and dApp error surfaces.
//
// NOTE: Soroban's `#[contracterror]` requires simple u32 repr
// (no data fields on the enum itself at the ABI level); we carry
// context via a companion `ErrorContext` struct that is logged
// and optionally emitted as an event, giving rich diagnostics
// without breaking the on-chain ABI.

#![no_std]
use soroban_sdk::{contracttype, contracterror, log, symbol_short, Address, Env};

// ─────────────────────────────────────────────────────────────
// Nebula Generation Errors
// ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NebulaGenError {
    /// ship_id must be > 0. Received ship_id=0.
    InvalidShipId      = 1,
    /// region_id must be in [1, MAX_REGION_ID].
    InvalidRegionId    = 2,
    /// seed cannot be all-zero bytes.
    InvalidSeed        = 3,
    /// No nebula layout exists for the given ship_id.
    LayoutNotFound     = 4,
    /// anomaly_index >= layout.size.
    AnomalyOutOfBounds = 5,
}

/// Companion context emitted to events / logs alongside NebulaGenError.
#[contracttype]
#[derive(Clone, Debug)]
pub struct NebulaGenErrorContext {
    pub error:      NebulaGenError,
    /// The ship_id that triggered the error (0 if not applicable).
    pub ship_id:    u64,
    /// The region_id that triggered the error (0 if not applicable).
    pub region_id:  u64,
    /// Human-readable description string (logged, not on-chain).
    pub detail:     &'static str,
}

impl NebulaGenError {
    /// Build a context struct for this error with relevant IDs.
    pub fn with_context(self, ship_id: u64, region_id: u64) -> NebulaGenErrorContext {
        let detail = match self {
            NebulaGenError::InvalidShipId =>
                "ship_id must be > 0; received 0",
            NebulaGenError::InvalidRegionId =>
                "region_id must be in [1, 1_000_000]; value is out of range",
            NebulaGenError::InvalidSeed =>
                "seed must not be all-zero bytes",
            NebulaGenError::LayoutNotFound =>
                "no nebula layout stored for this ship_id; call generate_nebula_layout first",
            NebulaGenError::AnomalyOutOfBounds =>
                "anomaly_index >= layout.size; check DEFAULT_ANOMALY_COUNT",
        };
        NebulaGenErrorContext { error: self, ship_id, region_id, detail }
    }

    /// Log context to the Soroban diagnostic log and return self.
    pub fn log_and_return(self, env: &Env, ship_id: u64, region_id: u64) -> Self {
        let ctx = self.with_context(ship_id, region_id);
        log!(
            env,
            "NebulaGenError {:?}: ship_id={} region_id={} — {}",
            ctx.error, ctx.ship_id, ctx.region_id, ctx.detail
        );
        self
    }
}

// ─────────────────────────────────────────────────────────────
// Rate Limiter Errors
// ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RateLimitError {
    /// Caller exceeded the allowed call rate for this operation.
    RateLimitExceeded = 100,
    /// Only the contract admin may update rate limit configuration.
    Unauthorized      = 101,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RateLimitErrorContext {
    pub error:      RateLimitError,
    /// Number of calls made in the current window.
    pub call_count: u32,
    /// Maximum calls allowed per window.
    pub max_calls:  u32,
    /// Window duration in seconds.
    pub window_secs: u64,
    pub detail:     &'static str,
}

impl RateLimitError {
    pub fn with_context(
        self,
        call_count: u32,
        max_calls: u32,
        window_secs: u64,
    ) -> RateLimitErrorContext {
        let detail = match self {
            RateLimitError::RateLimitExceeded =>
                "too many calls within the rate-limit window; retry after window_secs expires",
            RateLimitError::Unauthorized =>
                "caller is not the contract admin; admin auth required to change rate config",
        };
        RateLimitErrorContext { error: self, call_count, max_calls, window_secs, detail }
    }

    pub fn log_and_return(
        self,
        env: &Env,
        call_count: u32,
        max_calls: u32,
        window_secs: u64,
    ) -> Self {
        let ctx = self.with_context(call_count, max_calls, window_secs);
        log!(
            env,
            "RateLimitError {:?}: calls={}/{} window={}s — {}",
            ctx.error, ctx.call_count, ctx.max_calls, ctx.window_secs, ctx.detail
        );
        self
    }
}

// ─────────────────────────────────────────────────────────────
// Resource Minter Errors
// ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MinterError {
    /// amount must be > 0.
    InvalidAmount       = 200,
    /// Caller exceeded the minting rate limit.
    RateLimitExceeded   = 201,
    /// No nebula layout found for the given ship_id.
    NoLayoutForShip     = 202,
    /// The specified anomaly_index contains no mintable resource.
    NoResourceAtAnomaly = 203,
    /// Requested mint would overflow u64 total supply.
    SupplyOverflow      = 204,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MinterErrorContext {
    pub error:         MinterError,
    /// ship_id involved (0 if not applicable).
    pub ship_id:       u64,
    /// anomaly_index involved (u32::MAX if not applicable).
    pub anomaly_index: u32,
    /// Amount that was requested (0 if not applicable).
    pub amount:        u64,
    pub detail:        &'static str,
}

impl MinterError {
    pub fn with_context(self, ship_id: u64, anomaly_index: u32, amount: u64) -> MinterErrorContext {
        let detail = match self {
            MinterError::InvalidAmount =>
                "amount must be > 0; zero-amount mints are rejected",
            MinterError::RateLimitExceeded =>
                "minting rate limit exceeded; wait for the rolling window to expire",
            MinterError::NoLayoutForShip =>
                "no nebula layout found for ship_id; call generate_nebula_layout first",
            MinterError::NoResourceAtAnomaly =>
                "anomaly_index is out of bounds or does not contain a mintable resource",
            MinterError::SupplyOverflow =>
                "mint amount would overflow u64 total supply counter; reduce amount",
        };
        MinterErrorContext { error: self, ship_id, anomaly_index, amount, detail }
    }

    pub fn log_and_return(self, env: &Env, ship_id: u64, anomaly_index: u32, amount: u64) -> Self {
        let ctx = self.with_context(ship_id, anomaly_index, amount);
        log!(
            env,
            "MinterError {:?}: ship_id={} anomaly={} amount={} — {}",
            ctx.error, ctx.ship_id, ctx.anomaly_index, ctx.amount, ctx.detail
        );
        self
    }
}

// ─────────────────────────────────────────────────────────────
// Ship Registry Errors
// ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ShipRegistryError {
    /// ship_id is already registered; duplicate registration rejected.
    ShipAlreadyRegistered = 300,
    /// Ship not found for this ship_id.
    ShipNotFound          = 301,
    /// Only the ship owner may upgrade it.
    NotShipOwner          = 302,
    /// Ship level is already at the maximum allowed level.
    MaxLevelReached       = 303,
    /// ship name string exceeds the maximum allowed length.
    NameTooLong           = 304,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ShipRegistryErrorContext {
    pub error:      ShipRegistryError,
    pub ship_id:    u64,
    pub ship_level: u32,
    pub max_level:  u32,
    pub detail:     &'static str,
}

impl ShipRegistryError {
    pub fn with_context(
        self,
        ship_id:    u64,
        ship_level: u32,
        max_level:  u32,
    ) -> ShipRegistryErrorContext {
        let detail = match self {
            ShipRegistryError::ShipAlreadyRegistered =>
                "ship_id is already registered; each ID must be unique",
            ShipRegistryError::ShipNotFound =>
                "no ship found for ship_id; register the ship first",
            ShipRegistryError::NotShipOwner =>
                "caller is not the owner of this ship; only the owner may upgrade",
            ShipRegistryError::MaxLevelReached =>
                "ship is already at the maximum upgrade level",
            ShipRegistryError::NameTooLong =>
                "ship name exceeds the 64-character maximum",
        };
        ShipRegistryErrorContext { error: self, ship_id, ship_level, max_level, detail }
    }

    pub fn log_and_return(
        self,
        env: &Env,
        ship_id: u64,
        ship_level: u32,
        max_level: u32,
    ) -> Self {
        let ctx = self.with_context(ship_id, ship_level, max_level);
        log!(
            env,
            "ShipRegistryError {:?}: ship_id={} level={}/{} — {}",
            ctx.error, ctx.ship_id, ctx.ship_level, ctx.max_level, ctx.detail
        );
        self
    }
}

// ─────────────────────────────────────────────────────────────
// Nomad Bonding Errors
// ─────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BondingError {
    /// Bond already exists between these two addresses.
    BondAlreadyExists     = 400,
    /// Bond not found for the given pair.
    BondNotFound          = 401,
    /// Only bonded parties may interact with this bond.
    NotBondedParty        = 402,
    /// Bond is not in the expected state for this operation.
    InvalidBondState      = 403,
    /// yield_percentage must be in [1, 100].
    InvalidYieldPercent   = 404,
    /// Delegated yield amount exceeds the delegator's balance.
    InsufficientBalance   = 405,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct BondingErrorContext {
    pub error:           BondingError,
    /// Primary party address (initiator).
    pub party_a:         Address,
    /// Secondary party address (partner).
    pub party_b:         Address,
    /// Yield percentage involved (0 if not applicable).
    pub yield_percent:   u32,
    /// Amount involved (0 if not applicable).
    pub amount:          u64,
    pub detail:          &'static str,
}

impl BondingError {
    pub fn with_context(
        self,
        party_a:       Address,
        party_b:       Address,
        yield_percent: u32,
        amount:        u64,
    ) -> BondingErrorContext {
        let detail = match self {
            BondingError::BondAlreadyExists =>
                "a bond already exists between these two addresses; dissolve it first",
            BondingError::BondNotFound =>
                "no active bond found for this address pair; create_bond first",
            BondingError::NotBondedParty =>
                "caller is not one of the bonded parties; only party_a or party_b may act",
            BondingError::InvalidBondState =>
                "bond is not in the required state for this operation (e.g. must be Active)",
            BondingError::InvalidYieldPercent =>
                "yield_percentage must be between 1 and 100 inclusive",
            BondingError::InsufficientBalance =>
                "delegator does not have enough cosmic essence to cover the delegated yield",
        };
        BondingErrorContext { error: self, party_a, party_b, yield_percent, amount, detail }
    }
}

// ─────────────────────────────────────────────────────────────
// Tests — Issue #176
// ─────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nebula_error_context_carries_correct_ids() {
        let ctx = NebulaGenError::InvalidShipId.with_context(0, 42);
        assert_eq!(ctx.error, NebulaGenError::InvalidShipId);
        assert_eq!(ctx.ship_id, 0);
        assert_eq!(ctx.region_id, 42);
        assert!(!ctx.detail.is_empty());
    }

    #[test]
    fn minter_error_context_carries_amount() {
        let ctx = MinterError::InvalidAmount.with_context(7, 3, 0);
        assert_eq!(ctx.ship_id, 7);
        assert_eq!(ctx.anomaly_index, 3);
        assert_eq!(ctx.amount, 0);
    }

    #[test]
    fn ship_registry_error_max_level_context() {
        let ctx = ShipRegistryError::MaxLevelReached.with_context(55, 10, 10);
        assert_eq!(ctx.ship_level, ctx.max_level);
    }

    #[test]
    fn rate_limit_error_context_tracks_counts() {
        let ctx = RateLimitError::RateLimitExceeded.with_context(11, 10, 60);
        assert!(ctx.call_count > ctx.max_calls);
        assert_eq!(ctx.window_secs, 60);
    }

    #[test]
    fn all_nebula_errors_have_non_empty_detail() {
        use NebulaGenError::*;
        for err in [InvalidShipId, InvalidRegionId, InvalidSeed, LayoutNotFound, AnomalyOutOfBounds] {
            let ctx = err.with_context(1, 1);
            assert!(!ctx.detail.is_empty(), "{:?} has empty detail", err);
        }
    }

    #[test]
    fn all_minter_errors_have_non_empty_detail() {
        use MinterError::*;
        for err in [InvalidAmount, RateLimitExceeded, NoLayoutForShip, NoResourceAtAnomaly, SupplyOverflow] {
            let ctx = err.with_context(1, 0, 1);
            assert!(!ctx.detail.is_empty(), "{:?} has empty detail", err);
        }
    }
}
