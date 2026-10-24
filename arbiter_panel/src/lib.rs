#![no_std]

//! Vortex Protocol — m-of-n bonded arbiter panel (issues #407 / #555).
//!
//! Implements the specification in `docs/dispute-resolution-design.md`
//! ("Arbiter panel"): bonded arbiter registration, per-dispute panel
//! selection via `env.prng()`, public votes, m-of-n tally, bond slashing for
//! dissenting / absent arbiters, and the `VOTE_WINDOW` timeout fallback.
//!
//! `intent_settlement::resolve_dispute` accepts a resolution only from this
//! contract's address once configured via `set_arbiter_panel`.

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, panic_with_error, token,
    Address, Bytes, BytesN, Env, Map, Symbol, Vec,
};

#[cfg(test)]
mod test;

/// Voting window after `open_vote` (24 h).
pub const VOTE_WINDOW: u64 = 86_400;
/// Settlement's resolution window after a dispute is raised (24 h); mirrors
/// `intent_settlement::ARBITER_WINDOW`.
pub const ARBITER_WINDOW: u64 = 86_400;
/// Share of an arbiter's bond slashed when voting against the outcome or not
/// voting at all, in basis points (10%).
pub const SLASH_BPS: i128 = 1_000;

const TTL_THRESHOLD: u32 = 17_280 * 30;
const TTL_EXTEND_TO: u32 = 17_280 * 90;

/// Mirror of `intent_settlement::DisputeResolution` (same XDR encoding).
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisputeResolution {
    Upheld,
    Dismissed,
}

/// Subset of the settlement contract the panel calls.
#[contractclient(name = "SettlementClient")]
pub trait SettlementTrait {
    /// `(user, solver)` of an intent currently in the `Disputed` state.
    fn dispute_parties(env: Env, intent_id: BytesN<32>) -> (Address, Address);
    fn resolve_dispute(
        env: Env,
        arbiter: Address,
        intent_id: BytesN<32>,
        resolution: DisputeResolution,
    );
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    Settlement,
    BondToken,
    BondAmount,
    /// Configured panel size `n`.
    PanelSize,
    /// Configured threshold `m`.
    Threshold,
    /// All registered arbiters.
    Arbiters,
    Arbiter(Address),
    Round(BytesN<32>),
}

#[contracttype]
#[derive(Clone)]
pub struct ArbiterRecord {
    pub bond: i128,
    /// Number of unsettled rounds this arbiter was selected for.
    pub open_votes: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct Round {
    pub panel: Vec<Address>,
    /// Effective threshold after scaling for the actual panel size.
    pub threshold: u32,
    pub opened_at: u64,
    pub votes: Map<Address, DisputeResolution>,
    pub settled: bool,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidConfig = 3,
    AlreadyRegistered = 4,
    NotRegistered = 5,
    HasOpenVotes = 6,
    RoundExists = 7,
    RoundNotFound = 8,
    NoEligibleArbiters = 9,
    NotOnPanel = 10,
    AlreadyVoted = 11,
    VotingClosed = 12,
    RoundSettled = 13,
    QuorumNotReached = 14,
}

#[contract]
pub struct ArbiterPanel;

#[contractimpl]
impl ArbiterPanel {
    /// `threshold` (m) of `panel_size` (n) votes decide a dispute; `2m > n`
    /// is required so two conflicting outcomes cannot both reach quorum.
    pub fn initialize(
        env: Env,
        admin: Address,
        settlement: Address,
        bond_token: Address,
        bond_amount: i128,
        panel_size: u32,
        threshold: u32,
    ) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        if bond_amount <= 0 || threshold == 0 || threshold > panel_size || threshold * 2 <= panel_size
        {
            panic_with_error!(&env, Error::InvalidConfig);
        }
        let s = env.storage().instance();
        s.set(&DataKey::Admin, &admin);
        s.set(&DataKey::Settlement, &settlement);
        s.set(&DataKey::BondToken, &bond_token);
        s.set(&DataKey::BondAmount, &bond_amount);
        s.set(&DataKey::PanelSize, &panel_size);
        s.set(&DataKey::Threshold, &threshold);
        Self::bump_instance(&env);
    }

    /// Stake the bond and join the panel.
    pub fn register_arbiter(env: Env, arbiter: Address) {
        arbiter.require_auth();
        Self::bump_instance(&env);
        if env.storage().persistent().has(&DataKey::Arbiter(arbiter.clone())) {
            panic_with_error!(&env, Error::AlreadyRegistered);
        }
        let bond: i128 = Self::get(&env, &DataKey::BondAmount);
        token::Client::new(&env, &Self::get::<Address>(&env, &DataKey::BondToken)).transfer(
            &arbiter,
            &env.current_contract_address(),
            &bond,
        );
        Self::save_arbiter(&env, &arbiter, &ArbiterRecord { bond, open_votes: 0 });
        let mut list = Self::arbiters(&env);
        list.push_back(arbiter.clone());
        env.storage().instance().set(&DataKey::Arbiters, &list);
        env.events().publish((Symbol::new(&env, "arb_reg"),), arbiter);
    }

    /// Leave the panel and reclaim the remaining bond. Blocked while the
    /// arbiter has unsettled votes.
    pub fn deregister_arbiter(env: Env, arbiter: Address) {
        arbiter.require_auth();
        Self::bump_instance(&env);
        let rec = Self::load_arbiter(&env, &arbiter);
        if rec.open_votes > 0 {
            panic_with_error!(&env, Error::HasOpenVotes);
        }
        env.storage().persistent().remove(&DataKey::Arbiter(arbiter.clone()));
        let mut kept = Vec::new(&env);
        for a in Self::arbiters(&env).iter() {
            if a != arbiter {
                kept.push_back(a);
            }
        }
        env.storage().instance().set(&DataKey::Arbiters, &kept);
        if rec.bond > 0 {
            token::Client::new(&env, &Self::get::<Address>(&env, &DataKey::BondToken)).transfer(
                &env.current_contract_address(),
                &arbiter,
                &rec.bond,
            );
        }
        env.events().publish((Symbol::new(&env, "arb_dereg"),), arbiter);
    }

    /// Select the panel for a disputed intent and open the voting window.
    /// Permissionless. The intent's user and solver are excluded; the panel
    /// is `min(n, eligible)` arbiters and the threshold is scaled up-rounded
    /// proportionally (never below 1).
    pub fn open_vote(env: Env, intent_id: BytesN<32>) -> Vec<Address> {
        Self::bump_instance(&env);
        let key = DataKey::Round(intent_id.clone());
        if env.storage().persistent().has(&key) {
            panic_with_error!(&env, Error::RoundExists);
        }
        let settlement = SettlementClient::new(&env, &Self::get(&env, &DataKey::Settlement));
        let (user, solver) = settlement.dispute_parties(&intent_id);

        let bond_amount: i128 = Self::get(&env, &DataKey::BondAmount);
        let mut eligible: Vec<Address> = Vec::new(&env);
        for a in Self::arbiters(&env).iter() {
            if a != user && a != solver && Self::load_arbiter(&env, &a).bond >= bond_amount {
                eligible.push_back(a);
            }
        }
        if eligible.is_empty() {
            panic_with_error!(&env, Error::NoEligibleArbiters);
        }

        let n: u32 = Self::get(&env, &DataKey::PanelSize);
        let m: u32 = Self::get(&env, &DataKey::Threshold);
        let k = n.min(eligible.len());

        // Partial Fisher-Yates driven by the ledger PRNG seeded with the
        // intent id. Random-ish only (see design doc: PRNG limitation).
        let prng = env.prng();
        prng.seed(Bytes::from_array(&env, &intent_id.to_array()));
        let mut panel = Vec::new(&env);
        for i in 0..k {
            let j = prng.gen_range::<u64>(i as u64..=(eligible.len() - 1) as u64) as u32;
            let (a, b) = (eligible.get_unchecked(i), eligible.get_unchecked(j));
            eligible.set(i, b.clone());
            eligible.set(j, a);
            panel.push_back(b.clone());
            let mut rec = Self::load_arbiter(&env, &b);
            rec.open_votes += 1;
            Self::save_arbiter(&env, &b, &rec);
        }

        let threshold = ((m * k + n - 1) / n).max(1);
        let round = Round {
            panel: panel.clone(),
            threshold,
            opened_at: env.ledger().timestamp(),
            votes: Map::new(&env),
            settled: false,
        };
        Self::save_round(&env, &intent_id, &round);
        env.events()
            .publish((Symbol::new(&env, "vote_open"), intent_id), panel.clone());
        panel
    }

    /// Record a public vote. Each selected arbiter votes once, within
    /// `VOTE_WINDOW`.
    pub fn vote(env: Env, arbiter: Address, intent_id: BytesN<32>, resolution: DisputeResolution) {
        arbiter.require_auth();
        Self::bump_instance(&env);
        let mut round = Self::load_round(&env, &intent_id);
        if round.settled {
            panic_with_error!(&env, Error::RoundSettled);
        }
        if env.ledger().timestamp() >= round.opened_at + VOTE_WINDOW {
            panic_with_error!(&env, Error::VotingClosed);
        }
        if !round.panel.contains(&arbiter) {
            panic_with_error!(&env, Error::NotOnPanel);
        }
        if round.votes.contains_key(arbiter.clone()) {
            panic_with_error!(&env, Error::AlreadyVoted);
        }
        round.votes.set(arbiter.clone(), resolution);
        Self::save_round(&env, &intent_id, &round);
        env.events()
            .publish((Symbol::new(&env, "voted"), intent_id, arbiter), resolution);
    }

    /// Count votes. When `m` agree the panel resolves the dispute on the
    /// settlement contract, slashes dissenting / absent arbiters and returns
    /// the outcome. If the window elapsed without quorum the round is closed
    /// without slashing (so arbiters can deregister) and `None` is returned —
    /// settlement's `ARBITER_WINDOW` timeout rule then applies.
    pub fn tally(env: Env, intent_id: BytesN<32>) -> Option<DisputeResolution> {
        Self::bump_instance(&env);
        let mut round = Self::load_round(&env, &intent_id);
        if round.settled {
            panic_with_error!(&env, Error::RoundSettled);
        }
        let (mut upheld, mut dismissed) = (0u32, 0u32);
        for (_, v) in round.votes.iter() {
            match v {
                DisputeResolution::Upheld => upheld += 1,
                DisputeResolution::Dismissed => dismissed += 1,
            }
        }
        let outcome = if upheld >= round.threshold {
            Some(DisputeResolution::Upheld)
        } else if dismissed >= round.threshold {
            Some(DisputeResolution::Dismissed)
        } else {
            None
        };
        if outcome.is_none() && env.ledger().timestamp() < round.opened_at + VOTE_WINDOW {
            panic_with_error!(&env, Error::QuorumNotReached);
        }

        round.settled = true;
        Self::save_round(&env, &intent_id, &round);

        // Release selection locks; slash only when an outcome was reached.
        let mut winners: Vec<Address> = Vec::new(&env);
        let mut slashed_total: i128 = 0;
        for a in round.panel.iter() {
            let mut rec = Self::load_arbiter(&env, &a);
            rec.open_votes = rec.open_votes.saturating_sub(1);
            if let Some(o) = outcome {
                if round.votes.get(a.clone()) == Some(o) {
                    winners.push_back(a.clone());
                } else {
                    let cut = rec.bond * SLASH_BPS / 10_000;
                    rec.bond -= cut;
                    slashed_total += cut;
                }
            }
            Self::save_arbiter(&env, &a, &rec);
        }
        // Slashed bond is shared equally by the majority; dust goes to the first.
        if !winners.is_empty() && slashed_total > 0 {
            let share = slashed_total / winners.len() as i128;
            let dust = slashed_total - share * winners.len() as i128;
            for (i, a) in winners.iter().enumerate() {
                let mut rec = Self::load_arbiter(&env, &a);
                rec.bond += share + if i == 0 { dust } else { 0 };
                Self::save_arbiter(&env, &a, &rec);
            }
        }

        match outcome {
            Some(o) => {
                SettlementClient::new(&env, &Self::get(&env, &DataKey::Settlement))
                    .resolve_dispute(&env.current_contract_address(), &intent_id, &o);
                env.events().publish((Symbol::new(&env, "tallied"), intent_id), o);
            }
            None => env
                .events()
                .publish((Symbol::new(&env, "vote_expired"),), intent_id),
        }
        outcome
    }

    pub fn get_round(env: Env, intent_id: BytesN<32>) -> Option<Round> {
        env.storage().persistent().get(&DataKey::Round(intent_id))
    }

    pub fn get_arbiter(env: Env, arbiter: Address) -> Option<ArbiterRecord> {
        env.storage().persistent().get(&DataKey::Arbiter(arbiter))
    }

    pub fn list_arbiters(env: Env) -> Vec<Address> {
        Self::arbiters(&env)
    }

    fn get<T: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>>(env: &Env, key: &DataKey) -> T {
        env.storage()
            .instance()
            .get(key)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized))
    }

    fn arbiters(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::Arbiters)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn load_arbiter(env: &Env, a: &Address) -> ArbiterRecord {
        env.storage()
            .persistent()
            .get(&DataKey::Arbiter(a.clone()))
            .unwrap_or_else(|| panic_with_error!(env, Error::NotRegistered))
    }

    fn save_arbiter(env: &Env, a: &Address, rec: &ArbiterRecord) {
        let key = DataKey::Arbiter(a.clone());
        env.storage().persistent().set(&key, rec);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    fn load_round(env: &Env, id: &BytesN<32>) -> Round {
        let key = DataKey::Round(id.clone());
        let r = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, Error::RoundNotFound));
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        r
    }

    fn save_round(env: &Env, id: &BytesN<32>, r: &Round) {
        let key = DataKey::Round(id.clone());
        env.storage().persistent().set(&key, r);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }
}
