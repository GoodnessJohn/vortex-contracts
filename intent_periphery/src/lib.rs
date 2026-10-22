//! Vortex Protocol — Intent Periphery Contract (issue #376)
//!
//! Moves heavy view/governance surfaces out of the core settlement contract so
//! the core wasm stays small, cheap to load, and easy to audit.
//!
//! ## Design rationale
//!
//! Soroban's on-chain wasm limit is 64 KiB (65 536 bytes). The core
//! `intent_settlement` contract absorbs lifecycle, bonds, and slashing —
//! code that directly moves funds. Every KiB of admin/view logic added to
//! core grows the code-load fee on *every* user call.
//!
//! `intent_periphery` reads state from core via cross-contract view calls and
//! aggregates or re-exposes it to dashboards and indexers. Because it never
//! holds funds or changes core state, its attack surface is minimal and it can
//! be upgraded independently.
//!
//! ## Authenticated interface
//!
//! The periphery stores the settlement contract address set at `initialize`.
//! All read functions accept an explicit `settlement` parameter so callers can
//! verify which core they are reading from, and so the periphery can be pointed
//! at a testnet or a new core address without re-deploying.
//!
//! ## Interface-version handshake (issue #560)
//!
//! Cooperating contracts expose `get_interface_version() -> u32`. Callers that
//! depend on cross-contract calls verify the callee's version once, at
//! configuration time (`initialize` / `set_settlement`), rather than on every
//! call — so a post-upgrade mismatch is caught immediately without adding
//! per-call overhead. `INTERFACE_VERSION` is the value this periphery expects
//! from the core settlement contract.
//!
//! ## Reentrancy note
//!
//! All functions here are read-only (no token transfers, no storage writes on
//! the core). Cross-contract calls to `settlement` are therefore safe: a
//! re-entrant call from the core back into the periphery would be a no-op
//! (no funds, no side effects).

#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, BytesN, Env,
    String, Symbol, Vec,
};

// ─── Interface version ────────────────────────────────────────────────────────

/// Interface version this periphery expects from the core settlement contract.
///
/// Bump this whenever the cross-contract ABI used by the proxies below changes
/// in a way that is not backward compatible. The handshake in `initialize` and
/// `set_settlement` rejects a core that reports a different value.
pub const INTERFACE_VERSION: u32 = 1;

// ─── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Settlement,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InterfaceVersionMismatch = 4,
}

// ─── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct IntentPeriphery;

#[contractimpl]
impl IntentPeriphery {
    // ── Initialization ────────────────────────────────────────────────────────

    /// One-time setup: record admin and the core settlement contract address.
    ///
    /// Performs the interface-version handshake against `settlement` before
    /// persisting it, so a mismatched core is rejected at configuration time.
    pub fn initialize(env: Env, admin: Address, settlement: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        admin.require_auth();
        Self::verify_interface_version(&env, &settlement);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        env.events().publish(
            (Symbol::new(&env, "periphery_initialized"),),
            (admin, settlement),
        );
    }

    /// Admin-only: point the periphery at a new core settlement contract.
    ///
    /// Performs the interface-version handshake against `settlement` before
    /// persisting it, so a mismatched core is rejected at configuration time.
    pub fn set_settlement(env: Env, settlement: Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized));
        admin.require_auth();
        Self::verify_interface_version(&env, &settlement);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        env.events()
            .publish((Symbol::new(&env, "settlement_updated"),), settlement);
    }

    /// Return the core settlement contract this periphery reads from.
    pub fn get_settlement(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Settlement)
    }

    /// Interface-version handshake view (issue #560).
    ///
    /// Cooperating contracts expose this so callers can verify compatibility
    /// once at configuration time. Returns the version this periphery speaks.
    pub fn get_interface_version(_env: Env) -> u32 {
        INTERFACE_VERSION
    }

    // ── Heavy view proxies (issue #376) ───────────────────────────────────────
    //
    // Each function below is a thin proxy: it calls the equivalent view on the
    // core contract and returns its result unchanged. Keeping these calls in the
    // periphery means future changes to argument lists or return types only
    // require an upgrade of the (cheaper, less critical) periphery, not the core.

    /// Proxy: `settlement.get_stats()` — protocol-level aggregate counters.
    ///
    /// Returns the raw val from the cross-contract call so callers decode it
    /// using the core's published ABI. This avoids duplicating struct definitions
    /// here and keeps periphery upgrades independent of struct field additions.
    pub fn get_stats(env: Env) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "get_stats"),
            Vec::new(&env),
        )
    }

    /// Proxy: `settlement.list_solvers(start, limit)` — paginated solver list.
    pub fn list_solvers(env: Env, start: u32, limit: u32) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "list_solvers"),
            soroban_sdk::vec![&env, start.into(), limit.into()],
        )
    }

    /// Proxy: `settlement.list_open_intents(start, limit)` — paginated open-intent IDs.
    pub fn list_open_intents(env: Env, start: u32, limit: u32) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "list_open_intents"),
            soroban_sdk::vec![&env, start.into(), limit.into()],
        )
    }

    /// Proxy: `settlement.list_intents_by_user(user, start, limit)`.
    pub fn list_intents_by_user(
        env: Env,
        user: Address,
        start: u32,
        limit: u32,
    ) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "list_intents_by_user"),
            soroban_sdk::vec![&env, user.into_val(&env), start.into(), limit.into()],
        )
    }

    /// Proxy: `settlement.get_fee_schedule(solver)` — fee tiers for a solver.
    pub fn get_fee_schedule(env: Env, solver: Address) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "get_fee_schedule"),
            soroban_sdk::vec![&env, solver.into_val(&env)],
        )
    }

    /// Proxy: `settlement.get_solver(solver)` — full SolverRecord.
    pub fn get_solver(env: Env, solver: Address) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "get_solver"),
            soroban_sdk::vec![&env, solver.into_val(&env)],
        )
    }

    /// Proxy: `settlement.get_intent(intent_id)` — full IntentRecord.
    pub fn get_intent(env: Env, intent_id: BytesN<32>) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "get_intent"),
            soroban_sdk::vec![&env, intent_id.into_val(&env)],
        )
    }

    /// Proxy: `settlement.get_protocol_health()` — health/pause status.
    pub fn get_protocol_health(env: Env) -> soroban_sdk::Val {
        let settlement = Self::load_settlement(&env);
        env.invoke_contract(
            &settlement,
            &Symbol::new(&env, "get_protocol_health"),
            Vec::new(&env),
        )
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Query `settlement.get_interface_version()` and reject a mismatch.
    ///
    /// Called only at configuration time (`initialize` / `set_settlement`) so
    /// the extra cross-contract call is paid once, not on every proxy call.
    fn verify_interface_version(env: &Env, settlement: &Address) {
        let version: u32 = env.invoke_contract(
            settlement,
            &Symbol::new(env, "get_interface_version"),
            Vec::new(env),
        );
        if version != INTERFACE_VERSION {
            panic_with_error!(env, Error::InterfaceVersionMismatch);
        }
    }

    fn load_settlement(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Settlement)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }
}
