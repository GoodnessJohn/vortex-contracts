#![no_std]

//! Vortex Protocol Shared Utilities
//!
//! Common constants, helpers, and types used across all vortex contracts.
//! Eliminates duplication and ensures consistency (issue #352).

pub use soroban_sdk::{Address, Env};

// ─── Common Error Type ───────────────────────────────────────────────────────
// Issue #505: Named error type instead of `()` so callers can map directly.

/// Shared error type for `vortex-common` helpers.
#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CommonError {
  /// The caller is not the stored admin.
  Unauthorized = 1,
  /// The admin key is absent from storage (contract not initialized).
  NotInitialized = 2,
}

// ─── TTL Constants (Soroban Ledger Entry Lifecycle) ──────────────────────────
// Issue #352: Unified TTL constants prevent divergence between contracts

/// Approximate number of ledgers per day. Soroban has ~5s ledger close time.
pub const DAY_IN_LEDGERS: u32 = 17_280;

/// Threshold before persistent entries are archived by Soroban.
pub const PERSISTENT_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS * 14;

/// Duration to extend persistent TTL to (30 days).
pub const PERSISTENT_TTL_EXTEND_TO: u32 = DAY_IN_LEDGERS * 30;

/// Threshold for instance storage (contract metadata).
pub const INSTANCE_TTL_THRESHOLD: u32 = DAY_IN_LEDGERS * 30;

/// Duration to extend instance TTL to (60 days).
pub const INSTANCE_TTL_EXTEND_TO: u32 = DAY_IN_LEDGERS * 60;

// ─── Tier Constants ──────────────────────────────────────────────────────────
// Issue #352: Unified tier constants for reputation system

/// Maximum solver reputation tier (0 = Unranked, 1–5 are ranked tiers).
pub const MAX_TIER: u32 = 5;

/// Slash percentage in basis points per tier: tier N is slashed at (N × TIER_SLASH_BPS).
/// E.g., tier 3 at 500 bps = 5% slash.
pub const TIER_SLASH_BPS: i128 = 100; // 1% per tier

/// Basis point scale for all fractional arithmetic.
pub const BPS: i128 = 10_000;

// ─── Math Helpers ────────────────────────────────────────────────────────────

/// Multiply `amount` by `bps` (basis points) and divide by BPS (10_000).
/// #352: Checked math to prevent overflow on large amounts.
pub fn bps_mul_div(amount: i128, bps: i128) -> Option<i128> {
  amount
    .checked_mul(bps)?
    .checked_div(BPS)
}

/// Compute slash amount for a given tier (tier × TIER_SLASH_BPS of the principal).
/// Tier 0 (Unranked) = no slash.
pub fn compute_slash_bps(tier: u32) -> i128 {
  (tier as i128)
    .checked_mul(TIER_SLASH_BPS)
    .unwrap_or(0)
    .min(BPS)
}

// ─── Admin Pattern ───────────────────────────────────────────────────────────
// Issue #352: Two-step admin transfer is consistent across contracts

/// Storage key type for admin address (defined by contracts).
pub trait AdminStorage {
  fn get_admin(env: &Env) -> Address;
  fn set_admin(env: &Env, new_admin: Address);
}

/// Helper to assert the caller is admin.
///
/// Returns [`CommonError::NotInitialized`] if no admin is stored under
/// `admin_key`, and [`CommonError::Unauthorized`] if `caller` is not the admin.
pub fn require_admin(
  env: &Env,
  admin_key: impl soroban_sdk::IntoVal<Env>,
  caller: &Address,
) -> Result<(), CommonError> {
  let admin: Address = env
    .storage()
    .instance()
    .get(&admin_key)
    .ok_or(CommonError::NotInitialized)?;
  if admin != *caller {
    return Err(CommonError::Unauthorized);
  }
  Ok(())
}

// ─── Versioning for Storage Migrations (Issue #353) ──────────────────────────

/// Schema version for persistent records. Enables lazy migrations on first touch.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Base for version checking: starts at 1, increments for each migration.
pub trait Versioned {
  fn schema_version() -> u32;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_bps_mul_div() {
    // 1000 * 5000 BPS / 10_000 = 500
    assert_eq!(bps_mul_div(1000, 5000), Some(500));
    // Overflow case
    assert_eq!(bps_mul_div(i128::MAX, BPS + 1), None);
  }

  #[test]
  fn test_compute_slash_bps() {
    assert_eq!(compute_slash_bps(0), 0); // Unranked, no slash
    assert_eq!(compute_slash_bps(1), 100); // Tier 1 = 1%
    assert_eq!(compute_slash_bps(5), 500); // Tier 5 = 5%
    assert_eq!(compute_slash_bps(101), 10_000); // Capped at 100% (BPS)
  }

  #[test]
  fn test_ttl_constants() {
    assert!(PERSISTENT_TTL_THRESHOLD < PERSISTENT_TTL_EXTEND_TO);
    assert!(INSTANCE_TTL_THRESHOLD < INSTANCE_TTL_EXTEND_TO);
  }

  #[test]
  fn test_common_error_values() {
    assert_eq!(CommonError::Unauthorized as u32, 1);
    assert_eq!(CommonError::NotInitialized as u32, 2);
  }

  #[test]
  fn test_require_admin() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let other = Address::generate(&env);
    let key = soroban_sdk::symbol_short!("admin");

    // Missing admin key -> NotInitialized.
    assert_eq!(
      require_admin(&env, key.clone(), &admin),
      Err(CommonError::NotInitialized)
    );

    env.storage().instance().set(&key, &admin);

    // Matching caller -> Ok.
    assert_eq!(require_admin(&env, key.clone(), &admin), Ok(()));

    // Non-matching caller -> Unauthorized.
    assert_eq!(
      require_admin(&env, key, &other),
      Err(CommonError::Unauthorized)
    );
  }
}
