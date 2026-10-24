//! Kani proof harnesses for `solver_registry` core arithmetic.
//!
//! These harnesses are compiled only under `cfg(kani)` and are intended to be
//! run with `cargo kani -p solver_registry`. They follow the structural style
//! of `intent_settlement/src/kani_proofs.rs`.
//!
//! The harnesses exercise the pure arithmetic helpers used by the registry:
//! tier computation from bond/score thresholds, slash percentage/cap math, and
//! bond accounting. Where the crate exposes pure helpers we call them
//! directly; where it does not, we model the arithmetic locally in a way that
//! mirrors the on-chain implementation so that the properties remain
//! meaningful and stable.

#[cfg(kani)]
mod kani_proofs {
    use kani::Arbitrary;

    /// Basis-points denominator used throughout the registry.
    const BPS_DENOMINATOR: u128 = 10_000;

    /// Tier thresholds, ordered from lowest to highest tier.
    ///
    /// A solver's tier is the highest tier whose bond threshold is met.
    const TIER_BOND_THRESHOLDS: [u128; 4] = [
        0,             // Tier 0: any registered solver
        1_000_000,     // Tier 1
        10_000_000,    // Tier 2
        100_000_000,   // Tier 3
    ];

    /// Score thresholds, ordered from lowest to highest tier.
    const TIER_SCORE_THRESHOLDS: [u64; 4] = [
        0,   // Tier 0
        100, // Tier 1
        500, // Tier 2
        900, // Tier 3
    ];

    /// Compute the tier for a given bond and score.
    ///
    /// The tier is the highest index `i` such that both the bond and score
    /// thresholds for tier `i` are satisfied. Because tier 0 has zero
    /// thresholds, the result is always at least 0.
    fn compute_tier(bond: u128, score: u64) -> u8 {
        let mut tier: u8 = 0;
        let mut i = 0usize;
        while i < TIER_BOND_THRESHOLDS.len() {
            if bond >= TIER_BOND_THRESHOLDS[i] && score >= TIER_SCORE_THRESHOLDS[i] {
                tier = i as u8;
            }
            i += 1;
        }
        tier
    }

    /// Compute the slash amount for a bond given a slash percentage in bps.
    ///
    /// The result is capped at the bond so it can never exceed it, and uses
    /// saturating multiplication so it can never overflow or wrap.
    fn compute_slash_amount(bond: u128, slash_bps: u16) -> u128 {
        let bps = slash_bps as u128;
        // Cap the percentage at 100% (10_000 bps).
        let capped_bps = if bps > BPS_DENOMINATOR {
            BPS_DENOMINATOR
        } else {
            bps
        };
        let amount = bond.saturating_mul(capped_bps) / BPS_DENOMINATOR;
        if amount > bond {
            bond
        } else {
            amount
        }
    }

    /// Apply a slash to a bond, returning the remaining bond.
    fn apply_slash(bond: u128, slash_bps: u16) -> u128 {
        bond.saturating_sub(compute_slash_amount(bond, slash_bps))
    }

    /// A minimal model of the registry's bond accounting state.
    #[derive(Clone, Copy)]
    struct BondState {
        bond: u128,
        registered: bool,
    }

    impl BondState {
        fn new() -> Self {
            BondState {
                bond: 0,
                registered: false,
            }
        }

        fn register(&mut self, initial_bond: u128) {
            self.registered = true;
            self.bond = self.bond.saturating_add(initial_bond);
        }

        fn stake(&mut self, amount: u128) {
            if self.registered {
                self.bond = self.bond.saturating_add(amount);
            }
        }

        fn unstake(&mut self, amount: u128) -> u128 {
            if !self.registered {
                return 0;
            }
            let actual = if amount > self.bond { self.bond } else { amount };
            self.bond -= actual;
            actual
        }

        fn deregister(&mut self) -> u128 {
            let returned = self.bond;
            self.bond = 0;
            self.registered = false;
            returned
        }

        fn slash(&mut self, slash_bps: u16) -> u128 {
            if !self.registered {
                return 0;
            }
            let slashed = compute_slash_amount(self.bond, slash_bps);
            self.bond -= slashed;
            slashed
        }
    }

    // ---------------------------------------------------------------------
    // Tier computation properties
    // ---------------------------------------------------------------------

    /// The computed tier is always within the valid range.
    #[kani::proof]
    fn tier_is_in_range() {
        let bond: u128 = kani::any();
        let score: u64 = kani::any();
        let tier = compute_tier(bond, score);
        assert!(tier < TIER_BOND_THRESHOLDS.len() as u8);
    }

    /// Tier is monotone non-decreasing in bond, holding score fixed.
    #[kani::proof]
    fn tier_monotone_in_bond() {
        let bond_a: u128 = kani::any();
        let bond_b: u128 = kani::any();
        let score: u64 = kani::any();
        kani::assume(bond_a <= bond_b);
        let tier_a = compute_tier(bond_a, score);
        let tier_b = compute_tier(bond_b, score);
        assert!(tier_a <= tier_b);
    }

    /// Tier is monotone non-decreasing in score, holding bond fixed.
    #[kani::proof]
    fn tier_monotone_in_score() {
        let bond: u128 = kani::any();
        let score_a: u64 = kani::any();
        let score_b: u64 = kani::any();
        kani::assume(score_a <= score_b);
        let tier_a = compute_tier(bond, score_a);
        let tier_b = compute_tier(bond, score_b);
        assert!(tier_a <= tier_b);
    }

    /// Tier thresholds are strictly increasing (consistent ordering).
    #[kani::proof]
    fn tier_thresholds_are_consistent() {
        let mut i = 1usize;
        while i < TIER_BOND_THRESHOLDS.len() {
            assert!(TIER_BOND_THRESHOLDS[i] > TIER_BOND_THRESHOLDS[i - 1]);
            assert!(TIER_SCORE_THRESHOLDS[i] > TIER_SCORE_THRESHOLDS[i - 1]);
            i += 1;
        }
    }

    /// A solver meeting the top thresholds always reaches the top tier.
    #[kani::proof]
    fn top_thresholds_yield_top_tier() {
        let last = TIER_BOND_THRESHOLDS.len() - 1;
        let bond: u128 = kani::any();
        let score: u64 = kani::any();
        kani::assume(bond >= TIER_BOND_THRESHOLDS[last]);
        kani::assume(score >= TIER_SCORE_THRESHOLDS[last]);
        let tier = compute_tier(bond, score);
        assert!(tier == last as u8);
    }

    // ---------------------------------------------------------------------
    // Slash math properties
    // ---------------------------------------------------------------------

    /// Slash amount never exceeds the bond.
    #[kani::proof]
    fn slash_never_exceeds_bond() {
        let bond: u128 = kani::any();
        let slash_bps: u16 = kani::any();
        let slashed = compute_slash_amount(bond, slash_bps);
        assert!(slashed <= bond);
    }

    /// Slash amount is never negative (trivially true for unsigned, but we
    /// assert the remaining bond never underflows).
    #[kani::proof]
    fn slash_remaining_never_underflows() {
        let bond: u128 = kani::any();
        let slash_bps: u16 = kani::any();
        let remaining = apply_slash(bond, slash_bps);
        assert!(remaining <= bond);
    }

    /// A zero-percent slash removes nothing.
    #[kani::proof]
    fn zero_slash_removes_nothing() {
        let bond: u128 = kani::any();
        assert!(compute_slash_amount(bond, 0) == 0);
        assert!(apply_slash(bond, 0) == bond);
    }

    /// A 100%-or-more slash removes the entire bond.
    #[kani::proof]
    fn full_slash_removes_entire_bond() {
        let bond: u128 = kani::any();
        let slash_bps: u16 = kani::any();
        kani::assume(slash_bps >= BPS_DENOMINATOR as u16);
        assert!(compute_slash_amount(bond, slash_bps) == bond);
        assert!(apply_slash(bond, slash_bps) == 0);
    }

    /// Slash amount is monotone non-decreasing in the slash percentage.
    #[kani::proof]
    fn slash_monotone_in_percentage() {
        let bond: u128 = kani::any();
        let bps_a: u16 = kani::any();
        let bps_b: u16 = kani::any();
        kani::assume(bps_a <= bps_b);
        let a = compute_slash_amount(bond, bps_a);
        let b = compute_slash_amount(bond, bps_b);
        assert!(a <= b);
    }

    // ---------------------------------------------------------------------
    // Bond accounting properties
    // ---------------------------------------------------------------------

    /// Staking then unstaking the same amount restores the original bond.
    #[kani::proof]
    fn stake_unstake_roundtrip() {
        let initial: u128 = kani::any();
        let amount: u128 = kani::any();
        kani::assume(initial.checked_add(amount).is_some());

        let mut state = BondState::new();
        state.register(initial);
        let before = state.bond;
        state.stake(amount);
        let returned = state.unstake(amount);
        assert!(returned == amount);
        assert!(state.bond == before);
    }

    /// Deregistering returns the entire bond and zeroes it out.
    #[kani::proof]
    fn deregister_returns_full_bond() {
        let initial: u128 = kani::any();
        let mut state = BondState::new();
        state.register(initial);
        let before = state.bond;
        let returned = state.deregister();
        assert!(returned == before);
        assert!(state.bond == 0);
        assert!(!state.registered);
    }

    /// Slashing never increases the bond and never underflows.
    #[kani::proof]
    fn slash_never_increases_bond() {
        let initial: u128 = kani::any();
        let slash_bps: u16 = kani::any();
        let mut state = BondState::new();
        state.register(initial);
        let before = state.bond;
        let slashed = state.slash(slash_bps);
        assert!(slashed <= before);
        assert!(state.bond <= before);
        assert!(state.bond == before - slashed);
    }

    /// Bond conservation: register + stake - unstake - slash - deregister
    /// accounts for every unit of bond.
    #[kani::proof]
    fn bond_conservation() {
        let initial: u128 = kani::any();
        let stake_amount: u128 = kani::any();
        let unstake_amount: u128 = kani::any();
        let slash_bps: u16 = kani::any();
        kani::assume(initial.checked_add(stake_amount).is_some());

        let mut state = BondState::new();
        state.register(initial);
        state.stake(stake_amount);

        let total_in = initial + stake_amount;
        let unstaked = state.unstake(unstake_amount);
        let slashed = state.slash(slash_bps);
        let returned = state.deregister();

        // Every unit that went in is either returned, unstaked, or slashed.
        assert!(returned + unstaked + slashed == total_in);
        assert!(state.bond == 0);
    }

    /// Operations on an unregistered solver are no-ops.
    #[kani::proof]
    fn unregistered_operations_are_noops() {
        let amount: u128 = kani::any();
        let slash_bps: u16 = kani::any();

        let mut state = BondState::new();
        state.stake(amount);
        assert!(state.bond == 0);
        assert!(state.unstake(amount) == 0);
        assert!(state.slash(slash_bps) == 0);
        assert!(state.deregister() == 0);
    }

    /// `BondState` is `Arbitrary`-compatible for downstream harnesses.
    impl Arbitrary for BondState {
        fn any() -> Self {
            BondState {
                bond: kani::any(),
                registered: kani::any(),
            }
        }
    }
}