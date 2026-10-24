//! Boundary-condition tests for `governor::cast_vote`'s voting-window check.
//!
//! Issue #537.
//!
//! `cast_vote` guards the voting window with
//!
//! ```text
//! if now > proposal.deadline { return Err(GovernorError::VotingClosed); }
//! ```
//!
//! — an **exclusive** upper bound. The deadline ledger is therefore the last
//! ledger on which a vote still counts, and `deadline + 1` is the first one on
//! which voting is closed. This mirrors `state_of`, which treats a proposal as
//! `Active` while `now <= proposal.deadline`.
//!
//! This repository has an established convention of testing exactly this
//! class of off-by-one risk for every deadline comparison (see closed issue
//! #211, "Boundary-condition audit and exhaustive test suite for deadline
//! inclusivity/exclusivity"). `governor` is a newer contract and had not
//! received the same treatment, so this file pins the boundary down
//! explicitly.
//!
//! ## Running
//!
//! ```bash
//! cd governor && cargo test --test cast_vote_deadline_boundary
//! ```

use governor::{
    Checkpoint, Governor, GovernorClient, GovernorConfig, GovernorError, ProposalState, VoteType,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, Symbol, Vec,
};

// ── Fixture constants ────────────────────────────────────────────────────────

/// Ledger sequence at which the proposal under test is created.
const SNAPSHOT: u32 = 100;

/// `voting_delay`: the proposal becomes votable immediately, so the deadline
/// arithmetic under test is not confounded by the start-of-active boundary.
const VOTING_DELAY: u32 = 0;

/// `voting_period`: the voting window is `VOTING_DELAY + VOTING_PERIOD`
/// ledgers wide, so the proposal's deadline is `SNAPSHOT + VOTING_PERIOD`.
const VOTING_PERIOD: u32 = 10;

/// The ledger at which voting closes. `cast_vote` must still accept a vote
/// here — this is the boundary case the issue is about.
const DEADLINE: u32 = SNAPSHOT + VOTING_DELAY + VOTING_PERIOD;

/// Voting power seeded for the proposer and the voter.
const POWER: i128 = 1_000;

/// `proposal_threshold`: seeded power is comfortably above it.
const PROPOSAL_THRESHOLD: i128 = 1;

/// `quorum`: irrelevant to `cast_vote`, but set so the config is coherent.
const QUORUM: i128 = 1;

// ── Fixture ──────────────────────────────────────────────────────────────────

struct Ctx {
    env: Env,
    contract_id: Address,
    registry: Address,
    proposer: Address,
    voter: Address,
    target: Address,
}

impl Ctx {
    fn client(&self) -> GovernorClient<'_> {
        GovernorClient::new(&self.env, &self.contract_id)
    }

    /// Advance the ledger to `sequence`.
    fn advance_to(&self, sequence: u32) {
        self.env.ledger().set_sequence_number(sequence);
    }

    /// Seed the registry's checkpoint history for `account` directly into the
    /// governor's own storage.
    ///
    /// `Governor::voting_power` reads checkpoints from the governor's storage
    /// under the key `(Symbol("checkpoint"), registry, account)` and binary
    /// searches it for the latest checkpoint at or before the requested
    /// ledger. Writing that key directly is what the registry integration
    /// would do, without standing up a second contract.
    fn seed_checkpoint(&self, account: &Address) {
        let checkpoints = Vec::from_slice(
            &self.env,
            &[Checkpoint {
                ledger: SNAPSHOT,
                power: POWER,
            }],
        );
        let key = (
            Symbol::new(&self.env, "checkpoint"),
            self.registry.clone(),
            account.clone(),
        );
        self.env.as_contract(&self.contract_id, || {
            self.env.storage().persistent().set(&key, &checkpoints);
        });
    }

    /// Create a proposal at `SNAPSHOT` and return its id.
    fn propose(&self) -> u32 {
        self.advance_to(SNAPSHOT);
        let function = Symbol::new(&self.env, "set_fee");
        let calldata = BytesN::<32>::from_array(&self.env, &[0u8; 32]);
        self.client()
            .propose(&self.proposer, &self.target, &function, &calldata)
    }

    /// A fresh voter with voting power already seeded.
    ///
    /// Needed by tests that cast more than one vote: `cast_vote` rejects a
    /// second vote from the same address with `AlreadyVoted`.
    fn fresh_voter(&self) -> Address {
        let voter = Address::generate(&self.env);
        self.seed_checkpoint(&voter);
        voter
    }

    /// Cast a `For` vote as `voter` and return the recorded power, or the
    /// contract's own error.
    fn cast_vote_as(&self, voter: &Address, proposal_id: u32) -> Result<i128, GovernorError> {
        match self
            .client()
            .try_cast_vote(voter, &proposal_id, &VoteType::For)
        {
            Ok(Ok(power)) => Ok(power),
            Ok(Err(e)) => panic!("unexpected conversion error: {e:?}"),
            Err(Ok(e)) => Err(e),
            Err(Err(e)) => panic!("unexpected invoke error: {e:?}"),
        }
    }

    /// Cast a `For` vote as the fixture's voter and return the recorded power,
    /// or the contract's own error.
    fn cast_vote(&self, proposal_id: u32) -> Result<i128, GovernorError> {
        self.cast_vote_as(&self.voter, proposal_id)
    }
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();

    let registry = Address::generate(&env);
    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    // `propose` rejects a self-target, so the target must be a different
    // address from the governor contract itself.
    let target = Address::generate(&env);

    let contract_id = env.register_contract(None, Governor);

    let config = GovernorConfig {
        registry: registry.clone(),
        timelock: Address::generate(&env),
        voting_delay: VOTING_DELAY,
        voting_period: VOTING_PERIOD,
        proposal_threshold: PROPOSAL_THRESHOLD,
        quorum: QUORUM,
    };
    GovernorClient::new(&env, &contract_id).initialize(&config);

    let ctx = Ctx {
        env,
        contract_id,
        registry,
        proposer,
        voter,
        target,
    };
    ctx.seed_checkpoint(&ctx.proposer);
    ctx.seed_checkpoint(&ctx.voter);
    ctx
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// A vote cast at exactly `proposal.deadline` is **accepted**.
///
/// `cast_vote` rejects only when `now > deadline`, so the deadline ledger
/// itself is still inside the voting window.
#[test]
fn cast_vote_at_exact_deadline_succeeds() {
    let ctx = setup();
    let proposal_id = ctx.propose();

    ctx.advance_to(DEADLINE);

    let power = ctx
        .cast_vote(proposal_id)
        .expect("a vote at exactly the deadline must still be accepted");
    assert_eq!(power, POWER);

    // The vote is recorded, and the proposal is still `Active` on its
    // deadline ledger.
    let state = ctx.client().state(&proposal_id);
    assert_eq!(state, ProposalState::Active);
}

/// A vote cast at `proposal.deadline + 1` is **rejected** with
/// `GovernorError::VotingClosed`.
///
/// This is the first ledger past the deadline, so the voting window is
/// closed. The error must be `VotingClosed` specifically — not `AlreadyVoted`
/// or `NoVotingPower` — which proves the boundary check is what rejected it.
#[test]
fn cast_vote_one_ledger_after_deadline_is_rejected() {
    let ctx = setup();
    let proposal_id = ctx.propose();

    ctx.advance_to(DEADLINE + 1);

    let err = ctx
        .cast_vote(proposal_id)
        .expect_err("voting must be closed one ledger after the deadline");
    assert_eq!(err, GovernorError::VotingClosed);

    // The rejected vote must not have been recorded.
    let proposal = ctx.client().proposal(&proposal_id);
    assert_eq!(proposal.for_votes, 0);
}

/// The boundary is exactly where the implementation says it is: every ledger
/// up to and including the deadline accepts the vote, and every ledger past
/// it rejects it.
///
/// This walks the whole window rather than only its two endpoints, so an
/// off-by-one in either direction (an inclusive/exclusive mix-up, or a
/// comparison against `deadline + 1` / `deadline - 1`) cannot slip through.
///
/// Each ledger is voted on by a *different* voter, because `cast_vote` rejects
/// a second vote from the same address with `AlreadyVoted` — the point of the
/// test is the window, not the tally.
#[test]
fn cast_vote_window_is_inclusive_of_the_deadline_and_exclusive_after_it() {
    let ctx = setup();
    let proposal_id = ctx.propose();

    for sequence in SNAPSHOT..=DEADLINE {
        ctx.advance_to(sequence);
        let voter = ctx.fresh_voter();
        ctx.cast_vote_as(&voter, proposal_id)
            .unwrap_or_else(|e| panic!("vote at ledger {sequence} must be accepted, got {e:?}"));
    }

    ctx.advance_to(DEADLINE + 1);
    let voter = ctx.fresh_voter();
    let err = ctx
        .cast_vote_as(&voter, proposal_id)
        .expect_err("voting must be closed one ledger after the deadline");
    assert_eq!(err, GovernorError::VotingClosed);
}

/// The last ledger before the deadline is still open — the boundary is not
/// off by one in the *permissive* direction either.
#[test]
fn cast_vote_one_ledger_before_deadline_succeeds() {
    let ctx = setup();
    let proposal_id = ctx.propose();

    ctx.advance_to(DEADLINE - 1);

    let power = ctx
        .cast_vote(proposal_id)
        .expect("a vote one ledger before the deadline must be accepted");
    assert_eq!(power, POWER);
}

/// The proposal's deadline is the ledger the implementation computed, i.e.
/// `snapshot + voting_delay + voting_period` — the test fixture's `DEADLINE`
/// constant is not an independent guess.
#[test]
fn proposal_deadline_is_snapshot_plus_voting_delay_plus_voting_period() {
    let ctx = setup();
    let proposal_id = ctx.propose();

    let proposal = ctx.client().proposal(&proposal_id);
    assert_eq!(proposal.snapshot, SNAPSHOT);
    assert_eq!(proposal.deadline, DEADLINE);
    assert_eq!(proposal.deadline, SNAPSHOT + VOTING_DELAY + VOTING_PERIOD);
}
