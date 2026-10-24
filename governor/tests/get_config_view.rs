//! Tests for `governor::get_config` — Issue #532.
//!
//! `governor` stored its configuration under the private instance key
//! `"config"` and read it back through the private `Self::config` helper. No
//! public entrypoint returned it, so a UI, dashboard or off-chain tool could
//! not display the governor's own parameters — the registry it reads voting
//! power from, the timelock it queues into, the voting delay, the voting
//! period, the proposal threshold and the quorum — without guessing at
//! private storage layout.
//!
//! `get_config` exposes exactly that read.
//!
//! ## Running
//!
//! ```bash
//! cd governor && cargo test --test get_config_view
//! ```

use governor::{Checkpoint, Governor, GovernorClient, GovernorConfig, GovernorError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Symbol, Vec,
};

// ── Fixture constants ────────────────────────────────────────────────────────

/// Voting delay in ledgers.
const VOTING_DELAY: u32 = 5;

/// Voting period in ledgers.
const VOTING_PERIOD: u32 = 20;

/// Minimum voting power required to open a proposal.
const PROPOSAL_THRESHOLD: i128 = 1_000;

/// Minimum total voting power for a proposal to reach quorum.
const QUORUM: i128 = 5_000;

// ── Fixture ──────────────────────────────────────────────────────────────────

struct Ctx {
    env: Env,
    contract_id: Address,
    config: GovernorConfig,
}

impl Ctx {
    fn client(&self) -> GovernorClient<'_> {
        GovernorClient::new(&self.env, &self.contract_id)
    }

    /// Seed the registry's checkpoint history for `account` directly into the
    /// governor's own storage.
    ///
    /// `Governor::voting_power` reads checkpoints from the governor's storage
    /// under the key `(Symbol("checkpoint"), registry, account)`. Writing that
    /// key directly is what the registry integration would do, without
    /// standing up a second contract.
    fn seed_checkpoint(&self, account: &Address, ledger: u32, power: i128) {
        let checkpoints = Vec::from_slice(&self.env, &[Checkpoint { ledger, power }]);
        let key = (
            Symbol::new(&self.env, "checkpoint"),
            self.config.registry.clone(),
            account.clone(),
        );
        self.env.as_contract(&self.contract_id, || {
            self.env.storage().persistent().set(&key, &checkpoints);
        });
    }
}

/// A governor that has been initialized with the fixture configuration.
fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Governor);
    let config = GovernorConfig {
        registry: Address::generate(&env),
        timelock: Address::generate(&env),
        voting_delay: VOTING_DELAY,
        voting_period: VOTING_PERIOD,
        proposal_threshold: PROPOSAL_THRESHOLD,
        quorum: QUORUM,
    };
    GovernorClient::new(&env, &contract_id).initialize(&config);

    Ctx {
        env,
        contract_id,
        config,
    }
}

/// A governor that has been deployed but never initialized.
fn setup_uninitialized() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, Governor);
    let config = GovernorConfig {
        registry: Address::generate(&env),
        timelock: Address::generate(&env),
        voting_delay: VOTING_DELAY,
        voting_period: VOTING_PERIOD,
        proposal_threshold: PROPOSAL_THRESHOLD,
        quorum: QUORUM,
    };

    Ctx {
        env,
        contract_id,
        config,
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// The view returns the configuration the governor was initialized with, field
/// for field.
#[test]
fn get_config_returns_the_initialized_config() {
    let ctx = setup();

    let got = ctx.client().get_config();

    assert_eq!(got.registry, ctx.config.registry);
    assert_eq!(got.timelock, ctx.config.timelock);
    assert_eq!(got.voting_delay, ctx.config.voting_delay);
    assert_eq!(got.voting_period, ctx.config.voting_period);
    assert_eq!(got.proposal_threshold, ctx.config.proposal_threshold);
    assert_eq!(got.quorum, ctx.config.quorum);
}

/// The whole struct round-trips, so the view is not losing or reordering
/// anything.
#[test]
fn get_config_returns_the_whole_config_struct() {
    let ctx = setup();

    assert_eq!(ctx.client().get_config(), ctx.config);
}

/// Before initialization there is no configuration to return, and the view
/// says so with the same `NotInitialized` error every other entrypoint uses.
#[test]
fn get_config_fails_before_initialization() {
    let ctx = setup_uninitialized();

    let err = ctx
        .client()
        .try_get_config()
        .expect_err("an uninitialized governor has no configuration");
    assert_eq!(err, Ok(GovernorError::NotInitialized));
}

/// The view is read-only: calling it repeatedly returns the same value, and it
/// does not disturb the governor's own state.
#[test]
fn get_config_is_read_only() {
    let ctx = setup();

    let first = ctx.client().get_config();
    let second = ctx.client().get_config();
    let third = ctx.client().get_config();

    assert_eq!(first, ctx.config);
    assert_eq!(second, ctx.config);
    assert_eq!(third, ctx.config);
}

/// The configuration the view returns is the *live* one — the same values the
/// contract itself uses — not a stale or parallel copy.
///
/// `propose` computes a proposal's deadline as
/// `snapshot + voting_delay + voting_period`, so the deadline of a real
/// proposal cross-checks the two numbers the view reports.
#[test]
fn get_config_returns_the_configuration_the_contract_actually_uses() {
    let ctx = setup();

    let snapshot = 42u32;
    ctx.env.ledger().set_sequence_number(snapshot);

    let config = ctx.client().get_config();

    // Give the proposer enough voting power to open a proposal.
    let proposer = Address::generate(&ctx.env);
    ctx.seed_checkpoint(&proposer, snapshot, PROPOSAL_THRESHOLD);

    let function = Symbol::new(&ctx.env, "set_fee");
    let calldata = BytesN::<32>::from_array(&ctx.env, &[0u8; 32]);
    let target = Address::generate(&ctx.env);
    let proposal_id = ctx
        .client()
        .propose(&proposer, &target, &function, &calldata);
    let proposal = ctx.client().proposal(&proposal_id);

    // The deadline the contract computed, from the numbers the view reported.
    assert_eq!(proposal.snapshot, snapshot);
    assert_eq!(
        proposal.deadline,
        snapshot + config.voting_delay + config.voting_period
    );
    // …and the same thing from the fixture's constants, so the view is
    // reporting the configuration that was actually installed.
    assert_eq!(proposal.deadline, snapshot + VOTING_DELAY + VOTING_PERIOD);
}
