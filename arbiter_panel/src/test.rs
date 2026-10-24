extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    vec,
};

const BOND: i128 = 1_000;

// Minimal settlement stand-in recording the resolution it receives.
#[contract]
struct MockSettlement;

#[contracttype]
enum MockKey {
    Parties,
    Resolved,
    Caller,
}

#[contractimpl]
impl MockSettlement {
    pub fn set_parties(env: Env, user: Address, solver: Address) {
        env.storage().instance().set(&MockKey::Parties, &(user, solver));
    }
    pub fn dispute_parties(env: Env, _id: BytesN<32>) -> (Address, Address) {
        env.storage().instance().get(&MockKey::Parties).unwrap()
    }
    pub fn resolve_dispute(env: Env, arbiter: Address, _id: BytesN<32>, r: DisputeResolution) {
        arbiter.require_auth();
        env.storage().instance().set(&MockKey::Resolved, &r);
        env.storage().instance().set(&MockKey::Caller, &arbiter);
    }
    pub fn resolved(env: Env) -> Option<DisputeResolution> {
        env.storage().instance().get(&MockKey::Resolved)
    }
    pub fn caller(env: Env) -> Option<Address> {
        env.storage().instance().get(&MockKey::Caller)
    }
}

struct Ctx {
    env: Env,
    panel: ArbiterPanelClient<'static>,
    settle: MockSettlementClient<'static>,
    token: token::Client<'static>,
    arbiters: std::vec::Vec<Address>,
    id: BytesN<32>,
}

fn setup(n_arbiters: usize, n: u32, m: u32) -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let admin = Address::generate(&env);
    let tok = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let settle_id = env.register_contract(None, MockSettlement);
    let panel_id = env.register_contract(None, ArbiterPanel);
    let panel = ArbiterPanelClient::new(&env, &panel_id);
    panel.initialize(&admin, &settle_id, &tok, &BOND, &n, &m);
    let mut arbiters = std::vec::Vec::new();
    for _ in 0..n_arbiters {
        let a = Address::generate(&env);
        StellarAssetClient::new(&env, &tok).mint(&a, &BOND);
        panel.register_arbiter(&a);
        arbiters.push(a);
    }
    let settle = MockSettlementClient::new(&env, &settle_id);
    settle.set_parties(&Address::generate(&env), &Address::generate(&env));
    Ctx {
        token: token::Client::new(&env, &tok),
        id: BytesN::from_array(&env, &[7; 32]),
        env,
        panel,
        settle,
        arbiters,
    }
}

#[test]
fn bonding_and_deregistration() {
    let c = setup(2, 3, 2);
    let a = &c.arbiters[0];
    assert_eq!(c.token.balance(a), 0);
    assert_eq!(c.panel.get_arbiter(a).unwrap().bond, BOND);
    assert!(c.panel.try_register_arbiter(a).is_err());
    c.panel.deregister_arbiter(a);
    assert_eq!(c.token.balance(a), BOND);
    assert_eq!(c.panel.list_arbiters().len(), 1);
}

#[test]
fn invalid_config_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register_contract(None, ArbiterPanel);
    let p = ArbiterPanelClient::new(&env, &id);
    let a = Address::generate(&env);
    // 2m must exceed n
    assert!(p.try_initialize(&a, &a, &a, &BOND, &4, &2).is_err());
    assert!(p.try_initialize(&a, &a, &a, &0, &3, &2).is_err());
}

#[test]
fn majority_upholds_and_slashes_dissent_and_absent() {
    let c = setup(3, 3, 2);
    let panel = c.panel.open_vote(&c.id);
    assert_eq!(panel.len(), 3);
    assert_eq!(c.panel.get_round(&c.id).unwrap().threshold, 2);
    assert!(c.panel.try_tally(&c.id).is_err()); // no quorum yet, window open
    let (a, b, d) = (panel.get(0).unwrap(), panel.get(1).unwrap(), panel.get(2).unwrap());
    c.panel.vote(&a, &c.id, &DisputeResolution::Upheld);
    c.panel.vote(&b, &c.id, &DisputeResolution::Upheld);
    assert!(c.panel.try_vote(&a, &c.id, &DisputeResolution::Upheld).is_err());
    assert_eq!(c.panel.tally(&c.id), Some(DisputeResolution::Upheld));
    assert_eq!(c.settle.resolved(), Some(DisputeResolution::Upheld));
    assert_eq!(c.settle.caller(), Some(c.panel.address.clone()));
    // absent arbiter loses 10%, shared by the two majority voters
    assert_eq!(c.panel.get_arbiter(&d).unwrap().bond, 900);
    assert_eq!(c.panel.get_arbiter(&a).unwrap().bond, 1_050);
    assert_eq!(c.panel.get_arbiter(&b).unwrap().bond, 1_050);
    assert_eq!(c.panel.get_arbiter(&a).unwrap().open_votes, 0);
    assert!(c.panel.try_tally(&c.id).is_err()); // settled
}

#[test]
fn dissenter_is_slashed() {
    let c = setup(3, 3, 2);
    let panel = c.panel.open_vote(&c.id);
    let (a, b, d) = (panel.get(0).unwrap(), panel.get(1).unwrap(), panel.get(2).unwrap());
    c.panel.vote(&a, &c.id, &DisputeResolution::Dismissed);
    c.panel.vote(&d, &c.id, &DisputeResolution::Upheld);
    c.panel.vote(&b, &c.id, &DisputeResolution::Dismissed);
    assert_eq!(c.panel.tally(&c.id), Some(DisputeResolution::Dismissed));
    assert_eq!(c.panel.get_arbiter(&d).unwrap().bond, 900);
}

#[test]
fn user_and_solver_are_excluded() {
    let c = setup(3, 3, 2);
    c.settle.set_parties(&c.arbiters[0], &c.arbiters[1]);
    let panel = c.panel.open_vote(&c.id);
    assert_eq!(panel, vec![&c.env, c.arbiters[2].clone()]);
    // threshold scaled: ceil(2*1/3) = 1
    assert_eq!(c.panel.get_round(&c.id).unwrap().threshold, 1);
}

#[test]
fn no_eligible_arbiters_fails() {
    let c = setup(1, 3, 2);
    c.settle.set_parties(&c.arbiters[0], &Address::generate(&c.env));
    assert!(c.panel.try_open_vote(&c.id).is_err());
}

#[test]
fn round_cannot_be_opened_twice() {
    let c = setup(3, 3, 2);
    c.panel.open_vote(&c.id);
    assert!(c.panel.try_open_vote(&c.id).is_err());
}

#[test]
fn non_panel_member_cannot_vote_and_deregister_blocked() {
    let c = setup(3, 3, 2);
    let outsider = Address::generate(&c.env);
    let panel = c.panel.open_vote(&c.id);
    assert!(c.panel.try_vote(&outsider, &c.id, &DisputeResolution::Upheld).is_err());
    assert!(c.panel.try_deregister_arbiter(&panel.get(0).unwrap()).is_err());
}

#[test]
fn timeout_without_quorum_closes_round_without_slashing() {
    let c = setup(3, 3, 2);
    let panel = c.panel.open_vote(&c.id);
    c.panel.vote(&panel.get(0).unwrap(), &c.id, &DisputeResolution::Upheld);
    c.env.ledger().set_timestamp(1_000 + VOTE_WINDOW);
    assert!(c.panel.try_vote(&panel.get(1).unwrap(), &c.id, &DisputeResolution::Upheld).is_err());
    assert_eq!(c.panel.tally(&c.id), None);
    assert_eq!(c.settle.resolved(), None);
    for a in panel.iter() {
        let r = c.panel.get_arbiter(&a).unwrap();
        assert_eq!((r.bond, r.open_votes), (BOND, 0));
    }
}
