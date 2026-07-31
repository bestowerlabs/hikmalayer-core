//! Adversarial test suite.
//!
//! These are not feature tests. Each one plays an attacker with a specific
//! goal — mint supply, spend someone else's funds, replay a signature, halt a
//! node, drain a pool — and asserts the chain refuses.
//!
//! Everything here goes through the public API with real signatures, the same
//! way a hostile client would. Where a test funds an account it does so with a
//! signed transfer from the treasury, so no test can quietly grant itself a
//! capability a real attacker would not have.
//!
//! Run with `cargo test --release --test security` as well as in debug: the
//! release profile is what validators actually run, and several of these
//! attacks behave differently when integer overflow checks are off.

use hikmalayer::blockchain::state::{
    ChainState, AMM_POOL_ACCOUNT, MIN_VALIDATOR_STAKE, STAKING_POOL_ACCOUNT, TX_FEE,
    VESTING_POOL_ACCOUNT,
};
use hikmalayer::blockchain::transaction::{
    AmmAction, Transaction, TransactionType, UNITS_PER_HKM,
};
use hikmalayer::consensus::{pos, vrf};

const SUPPLY: u64 = 100_000_000 * UNITS_PER_HKM;

struct Account {
    address: String,
    public_key: String,
    private_key: String,
    vrf_public_key: String,
}

fn account(seed: u8) -> Account {
    let private_key = hex::encode([seed; 32]);
    let public_key = pos::derive_public_key(&private_key).unwrap();
    Account {
        address: pos::derive_address(&public_key).unwrap(),
        vrf_public_key: vrf::derive_vrf_public_key(&private_key).unwrap(),
        public_key,
        private_key,
    }
}

/// Genesis with `treasury` as the funded genesis validator.
fn chain() -> (ChainState, Account) {
    chain_named(hikmalayer::blockchain::state::DEFAULT_CHAIN_ID)
}

fn chain_named(chain_id: &str) -> (ChainState, Account) {
    let treasury = account(1);
    let state = ChainState::genesis_for_chain(
        chain_id,
        &treasury.address,
        Some(&treasury.public_key),
        Some(&treasury.vrf_public_key),
        SUPPLY,
        &[],
    );
    (state, treasury)
}

/// A signed transfer, exactly as a client would build it.
fn signed_transfer(from: &Account, to: &str, amount: u64, nonce: u64) -> Transaction {
    signed_transfer_on(
        hikmalayer::blockchain::state::DEFAULT_CHAIN_ID,
        from,
        to,
        amount,
        nonce,
    )
}

/// A transfer signed for a specific network.
fn signed_transfer_on(
    chain_id: &str,
    from: &Account,
    to: &str,
    amount: u64,
    nonce: u64,
) -> Transaction {
    let mut tx = Transaction::new(
        Some(from.address.clone()),
        to.to_string(),
        amount,
        TransactionType::Transfer,
    )
    .for_chain(chain_id);
    tx.nonce = nonce;
    tx.public_key = Some(from.public_key.clone());
    let base = Transaction::transfer_signing_message(&from.address, to, amount, nonce);
    let message = Transaction::scoped_signing_message(chain_id, &base);
    tx.signature = Some(pos::sign_message(&message, &from.private_key).unwrap());
    tx
}

/// Sign a transaction in place, scoped to the default network.
fn authorize(tx: &mut Transaction, base_message: &str, signer: &Account) {
    let chain_id = hikmalayer::blockchain::state::DEFAULT_CHAIN_ID;
    tx.chain_id = chain_id.to_string();
    let message = Transaction::scoped_signing_message(chain_id, base_message);
    tx.signature = Some(pos::sign_message(&message, &signer.private_key).unwrap());
}

/// Fund an account the honest way, so no test grants itself free money.
fn fund(state: &mut ChainState, treasury: &Account, to: &str, amount: u64, nonce: u64) {
    let tx = signed_transfer(treasury, to, amount, nonce);
    state
        .apply_transaction(&tx, 1)
        .expect("treasury funding should succeed");
}

/// Total HKM in existence, for supply-conservation checks.
fn hkm_total(state: &ChainState) -> u128 {
    // Every account the tests touch, plus the internal pools.
    let mut accounts: Vec<String> = (1..=20u8).map(|s| account(s).address).collect();
    accounts.push(STAKING_POOL_ACCOUNT.to_string());
    accounts.push(VESTING_POOL_ACCOUNT.to_string());
    accounts.push(AMM_POOL_ACCOUNT.to_string());
    accounts
        .iter()
        .map(|a| state.balance_of(a) as u128)
        .sum::<u128>()
        + state.fee_pot as u128
}

// ===================================================================
// Authorization
// ===================================================================

/// The signature must be bound to the account it spends from. Without that
/// binding, anyone could sign "move the treasury's money" with their own key.
#[test]
fn cannot_spend_from_an_account_you_do_not_control() {
    let (state, treasury) = chain();
    let attacker = account(2);
    let victim_before = state.balance_of(&treasury.address);

    // Attacker signs a transfer OUT OF the treasury, using their own key.
    let mut tx = Transaction::new(
        Some(treasury.address.clone()),
        attacker.address.clone(),
        1_000 * UNITS_PER_HKM,
        TransactionType::Transfer,
    );
    tx.nonce = 1;
    tx.public_key = Some(attacker.public_key.clone());
    let message = Transaction::transfer_signing_message(
        &treasury.address,
        &attacker.address,
        1_000 * UNITS_PER_HKM,
        1,
    );
    authorize(&mut tx, &message, &attacker);

    assert!(tx.verify_for_block("").is_err(), "forged sender accepted");
    assert_eq!(state.balance_of(&treasury.address), victim_before);
    assert_eq!(state.balance_of(&attacker.address), 0);
}

/// Swapping in someone else's public key must not work either: the address
/// derived from the key is what the chain trusts.
#[test]
fn cannot_substitute_a_public_key_for_another_account() {
    let (_, treasury) = chain();
    let attacker = account(2);

    let mut tx = signed_transfer(&attacker, &treasury.address, 1, 1);
    // Keep the attacker's valid signature, claim to be the treasury.
    tx.from = Some(treasury.address.clone());
    assert!(tx.verify_for_block("").is_err());

    // Or keep the sender, swap in the treasury's public key.
    let mut tx = signed_transfer(&attacker, &treasury.address, 1, 1);
    tx.public_key = Some(treasury.public_key.clone());
    assert!(tx.verify_for_block("").is_err());
}

/// A signature is spent once. Replaying it must fail on the nonce.
#[test]
fn a_signature_cannot_be_replayed() {
    let (mut state, treasury) = chain();
    let victim = account(2);
    let tx = signed_transfer(&treasury, &victim.address, 500 * UNITS_PER_HKM, 1);

    state.apply_transaction(&tx, 1).unwrap();
    let after_first = state.balance_of(&victim.address);

    // The identical transaction, submitted again.
    assert!(
        state.apply_transaction(&tx, 2).is_err(),
        "replayed transaction accepted"
    );
    assert_eq!(state.balance_of(&victim.address), after_first);
}

/// Nonces must be strictly sequential — no skipping ahead to reserve a slot,
/// no reusing an old one.
#[test]
fn nonces_must_be_sequential() {
    let (mut state, treasury) = chain();
    let victim = account(2);

    // Skipping ahead is refused.
    let ahead = signed_transfer(&treasury, &victim.address, UNITS_PER_HKM, 5);
    assert!(state.apply_transaction(&ahead, 1).is_err());

    // In order is fine.
    let first = signed_transfer(&treasury, &victim.address, UNITS_PER_HKM, 1);
    state.apply_transaction(&first, 1).unwrap();

    // Going back is refused.
    let reused = signed_transfer(&treasury, &victim.address, UNITS_PER_HKM, 1);
    assert!(state.apply_transaction(&reused, 2).is_err());
}

/// A transfer signature must not be reusable as any other operation. Each
/// canonical message carries its own domain prefix for exactly this reason.
#[test]
fn a_transfer_signature_cannot_authorize_a_stake() {
    let (mut state, treasury) = chain();
    let transfer = signed_transfer(&treasury, &account(2).address, MIN_VALIDATOR_STAKE, 1);

    let mut stake = Transaction::new(
        Some(treasury.address.clone()),
        treasury.address.clone(),
        MIN_VALIDATOR_STAKE,
        TransactionType::Stake,
    );
    stake.nonce = 1;
    stake.public_key = transfer.public_key.clone();
    stake.signature = transfer.signature.clone(); // the transfer's signature
    stake.vrf_public_key = Some(treasury.vrf_public_key.clone());

    assert!(stake.verify_for_block("").is_err(), "cross-domain replay");
    assert!(state.apply_transaction(&stake, 1).is_err());
}

// ===================================================================
// Supply and arithmetic
// ===================================================================

/// The attack that motivated this suite: choose `amount` so `amount + fee`
/// wraps, and the balance check passes while the full amount is credited.
#[test]
fn an_overflowing_transfer_cannot_mint_supply() {
    let (mut state, treasury) = chain();
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, TX_FEE * 100, 1);

    let supply_before = hkm_total(&state);
    let recipient = account(3);

    // `amount + base_fee` wraps to zero.
    let amount = u64::MAX - TX_FEE + 1;
    let tx = signed_transfer(&attacker, &recipient.address, amount, 1);

    assert!(
        state.apply_transaction(&tx, 1).is_err(),
        "overflowing transfer accepted — this mints supply from nothing"
    );
    assert_eq!(state.balance_of(&recipient.address), 0);
    assert_eq!(hkm_total(&state), supply_before, "supply changed");
}

/// The same trick aimed at the validator set. Free stake is control of
/// leader election, which is control of the chain.
#[test]
fn an_overflowing_stake_cannot_mint_voting_power() {
    let (mut state, treasury) = chain();
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, TX_FEE * 100, 1);

    let amount = u64::MAX - TX_FEE + 1;
    let mut tx = Transaction::new(
        Some(attacker.address.clone()),
        attacker.address.clone(),
        amount,
        TransactionType::Stake,
    );
    tx.nonce = 1;
    tx.public_key = Some(attacker.public_key.clone());
    tx.vrf_public_key = Some(attacker.vrf_public_key.clone());
    let message = Transaction::stake_signing_message(
        &attacker.address,
        amount,
        1,
        &attacker.vrf_public_key,
    );
    authorize(&mut tx, &message, &attacker);

    assert!(state.apply_transaction(&tx, 1).is_err());
    assert!(
        state.validator_set().iter().all(|v| v.address != attacker.address),
        "attacker joined the validator set for free"
    );
}

/// Spending more than you hold, by any route, must fail.
#[test]
fn cannot_spend_more_than_the_balance() {
    let (mut state, treasury) = chain();
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, 10 * UNITS_PER_HKM, 1);

    let supply_before = hkm_total(&state);
    let tx = signed_transfer(&attacker, &treasury.address, 1_000 * UNITS_PER_HKM, 1);
    assert!(state.apply_transaction(&tx, 1).is_err());
    assert_eq!(hkm_total(&state), supply_before);
}

/// The fee is not optional: an account with exactly the transfer amount and
/// nothing spare cannot pay, and the transaction must fail cleanly.
#[test]
fn a_transfer_must_cover_its_own_fee() {
    let (mut state, treasury) = chain();
    let payer = account(2);
    fund(&mut state, &treasury, &payer.address, 5 * UNITS_PER_HKM, 1);

    let balance = state.balance_of(&payer.address);
    let tx = signed_transfer(&payer, &treasury.address, balance, 1);
    assert!(
        state.apply_transaction(&tx, 1).is_err(),
        "spent the whole balance with no room for the fee"
    );
    assert_eq!(state.balance_of(&payer.address), balance);
}

/// Ordinary activity must never change how much HKM exists.
#[test]
fn ordinary_transfers_conserve_supply() {
    let (mut state, treasury) = chain();
    let a = account(2);
    let b = account(3);
    fund(&mut state, &treasury, &a.address, 1_000 * UNITS_PER_HKM, 1);

    let before = hkm_total(&state);
    for nonce in 1..=5 {
        let tx = signed_transfer(&a, &b.address, 10 * UNITS_PER_HKM, nonce);
        state.apply_transaction(&tx, 1).unwrap();
    }
    assert_eq!(hkm_total(&state), before, "supply moved during transfers");
}

// ===================================================================
// Recipient validation (the fund-loss class)
// ===================================================================

/// Balances are keyed by string. An unvalidated recipient means a typo
/// creates an account nobody can spend from — funds gone, no checksum to
/// catch it, no way to reverse it.
#[test]
fn funds_cannot_be_sent_to_a_malformed_address() {
    let (mut state, treasury) = chain();
    for bad in [
        "typo-address",
        "",
        "hkm",
        "hkmshort",
        "HKM13320761030A4C59D96060708E2377BC4E936DEE", // uppercase
        STAKING_POOL_ACCOUNT,                          // an internal account
        AMM_POOL_ACCOUNT,
    ] {
        let before = state.balance_of(&treasury.address);
        let tx = signed_transfer(&treasury, bad, UNITS_PER_HKM, 1);
        assert!(
            state.apply_transaction(&tx, 1).is_err(),
            "accepted a transfer to {bad:?}"
        );
        assert_eq!(
            state.balance_of(&treasury.address),
            before,
            "balance moved for {bad:?}"
        );
    }
}

/// An internal pool account must not be addressable by a user, or its
/// accounting stops matching the positions it is supposed to back.
#[test]
fn internal_pool_accounts_are_not_user_addressable() {
    let (mut state, treasury) = chain();
    let pool_before = state.balance_of(AMM_POOL_ACCOUNT);
    let tx = signed_transfer(&treasury, AMM_POOL_ACCOUNT, 1_000 * UNITS_PER_HKM, 1);
    assert!(state.apply_transaction(&tx, 1).is_err());
    assert_eq!(state.balance_of(AMM_POOL_ACCOUNT), pool_before);
}

// ===================================================================
// Staking
// ===================================================================

/// A trivial stake must not buy a seat in leader election.
#[test]
fn cannot_join_the_validator_set_below_the_minimum_stake() {
    let (mut state, treasury) = chain();
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, MIN_VALIDATOR_STAKE, 1);

    let amount = MIN_VALIDATOR_STAKE / 100;
    let mut tx = Transaction::new(
        Some(attacker.address.clone()),
        attacker.address.clone(),
        amount,
        TransactionType::Stake,
    );
    tx.nonce = 1;
    tx.public_key = Some(attacker.public_key.clone());
    tx.vrf_public_key = Some(attacker.vrf_public_key.clone());
    let message =
        Transaction::stake_signing_message(&attacker.address, amount, 1, &attacker.vrf_public_key);
    authorize(&mut tx, &message, &attacker);

    assert!(state.apply_transaction(&tx, 1).is_err());
    assert!(state.validator_set().iter().all(|v| v.address != attacker.address));
}

/// Withdrawing more stake than is bonded must fail — otherwise the staking
/// pool pays out money it never received.
#[test]
fn cannot_withdraw_more_stake_than_is_bonded() {
    let (mut state, treasury) = chain();
    let pool_before = state.balance_of(STAKING_POOL_ACCOUNT);

    let amount = pool_before + 1_000 * UNITS_PER_HKM;
    let mut tx = Transaction::new(
        Some(treasury.address.clone()),
        treasury.address.clone(),
        amount,
        TransactionType::Withdraw,
    );
    tx.nonce = 1;
    tx.public_key = Some(treasury.public_key.clone());
    let message = Transaction::withdraw_signing_message(&treasury.address, amount, 1);
    authorize(&mut tx, &message, &treasury);

    assert!(state.apply_transaction(&tx, 1).is_err());
    assert_eq!(state.balance_of(STAKING_POOL_ACCOUNT), pool_before);
}

// ===================================================================
// DEX
// ===================================================================

/// Build a pool the honest way so the DEX tests start from real state.
fn seed_pool(state: &mut ChainState, owner: &Account, nonce: &mut u64) -> String {
    // Issue a token.
    let mut create = Transaction::new(
        Some(owner.address.clone()),
        owner.address.clone(),
        0,
        TransactionType::TokenCreate,
    );
    create.nonce = *nonce;
    create.public_key = Some(owner.public_key.clone());
    create.token = Some(hikmalayer::blockchain::transaction::TokenAction {
        token_id: String::new(),
        symbol: "SEC".into(),
        name: "Security".into(),
        decimals: 6,
    });
    create.amount = 1_000_000 * UNITS_PER_HKM;
    let message = Transaction::token_create_signing_message(
        "SEC",
        "Security",
        6,
        create.amount,
        *nonce,
    );
    authorize(&mut create, &message, owner);
    let token_id = hikmalayer::blockchain::transaction::derive_token_id(
        &owner.address,
        "SEC",
        *nonce,
    );
    state.apply_transaction(&create, 1).expect("token creation");
    *nonce += 1;

    // Seed liquidity.
    let hkm = 10_000 * UNITS_PER_HKM;
    let token = 50_000 * UNITS_PER_HKM;
    let mut add = Transaction::new(
        Some(owner.address.clone()),
        owner.address.clone(),
        0,
        TransactionType::AddLiquidity,
    );
    add.nonce = *nonce;
    add.public_key = Some(owner.public_key.clone());
    add.amm = Some(AmmAction {
        token_id: token_id.clone(),
        amount_hkm: hkm,
        amount_token: token,
        min_shares: 1,
        ..Default::default()
    });
    let message =
        Transaction::amm_add_signing_message(&token_id, hkm, token, 1, *nonce);
    authorize(&mut add, &message, owner);
    state.apply_transaction(&add, 1).expect("add liquidity");
    *nonce += 1;

    token_id
}

/// A swap must never let the trader take out more than the curve allows.
/// `min_out` is the trader's own bound; the pool's protection is the
/// constant-product invariant, which must hold across every trade.
#[test]
fn swaps_preserve_the_constant_product_invariant() {
    let (mut state, treasury) = chain();
    let mut nonce = 1u64;
    let token_id = seed_pool(&mut state, &treasury, &mut nonce);

    let pool = state.pools.get(&token_id).unwrap().clone();
    let k_before = pool.reserve_hkm as u128 * pool.reserve_token as u128;

    let amount_in = 100 * UNITS_PER_HKM;
    let mut swap = Transaction::new(
        Some(treasury.address.clone()),
        treasury.address.clone(),
        0,
        TransactionType::Swap,
    );
    swap.nonce = nonce;
    swap.public_key = Some(treasury.public_key.clone());
    swap.amm = Some(AmmAction {
        token_id: token_id.clone(),
        hkm_to_token: true,
        amount_in,
        min_out: 1,
        ..Default::default()
    });
    let message =
        Transaction::amm_swap_signing_message(&token_id, true, amount_in, 1, nonce);
    authorize(&mut swap, &message, &treasury);
    state.apply_transaction(&swap, 1).unwrap();

    let after = state.pools.get(&token_id).unwrap();
    let k_after = after.reserve_hkm as u128 * after.reserve_token as u128;
    // The 0.30% fee stays in the pool, so k must GROW. If it ever shrinks,
    // value is leaking out of the pool to traders.
    assert!(k_after >= k_before, "constant product shrank: {k_before} -> {k_after}");
}

/// A slippage bound is part of the signed message. If the chain ignored it,
/// the protection users think they have would be imaginary.
#[test]
fn an_unsatisfiable_slippage_bound_rejects_the_swap() {
    let (mut state, treasury) = chain();
    let mut nonce = 1u64;
    let token_id = seed_pool(&mut state, &treasury, &mut nonce);
    let pool_before = state.pools.get(&token_id).unwrap().clone();

    let amount_in = 100 * UNITS_PER_HKM;
    let min_out = u64::MAX / 2; // impossible
    let mut swap = Transaction::new(
        Some(treasury.address.clone()),
        treasury.address.clone(),
        0,
        TransactionType::Swap,
    );
    swap.nonce = nonce;
    swap.public_key = Some(treasury.public_key.clone());
    swap.amm = Some(AmmAction {
        token_id: token_id.clone(),
        hkm_to_token: true,
        amount_in,
        min_out,
        ..Default::default()
    });
    let message =
        Transaction::amm_swap_signing_message(&token_id, true, amount_in, min_out, nonce);
    authorize(&mut swap, &message, &treasury);

    assert!(state.apply_transaction(&swap, 1).is_err());
    let after = state.pools.get(&token_id).unwrap();
    assert_eq!(after.reserve_hkm, pool_before.reserve_hkm);
    assert_eq!(after.reserve_token, pool_before.reserve_token);
}

/// Burning LP shares you do not hold must fail, or a pool can be drained by
/// anyone who can name it.
#[test]
fn cannot_burn_lp_shares_you_do_not_hold() {
    let (mut state, treasury) = chain();
    let mut nonce = 1u64;
    let token_id = seed_pool(&mut state, &treasury, &mut nonce);
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, 100 * UNITS_PER_HKM, nonce);

    let pool_before = state.pools.get(&token_id).unwrap().clone();
    let shares = pool_before.total_shares;

    let mut remove = Transaction::new(
        Some(attacker.address.clone()),
        attacker.address.clone(),
        0,
        TransactionType::RemoveLiquidity,
    );
    remove.nonce = 1;
    remove.public_key = Some(attacker.public_key.clone());
    remove.amm = Some(AmmAction {
        token_id: token_id.clone(),
        shares,
        min_hkm: 1,
        min_token: 1,
        ..Default::default()
    });
    let message =
        Transaction::amm_remove_signing_message(&token_id, shares, 1, 1, 1);
    authorize(&mut remove, &message, &attacker);

    assert!(state.apply_transaction(&remove, 1).is_err(), "pool drained");
    let after = state.pools.get(&token_id).unwrap();
    assert_eq!(after.reserve_hkm, pool_before.reserve_hkm);
}

/// The locked MINIMUM_LIQUIDITY must keep `total_shares` above zero forever.
/// If it could reach zero, the next depositor would reset the pool's price
/// and the share-inflation attack it prevents comes back.
#[test]
fn total_shares_can_never_reach_zero() {
    let (mut state, treasury) = chain();
    let mut nonce = 1u64;
    let token_id = seed_pool(&mut state, &treasury, &mut nonce);

    let held = state.lp_shares_of(&token_id, &treasury.address);
    let mut remove = Transaction::new(
        Some(treasury.address.clone()),
        treasury.address.clone(),
        0,
        TransactionType::RemoveLiquidity,
    );
    remove.nonce = nonce;
    remove.public_key = Some(treasury.public_key.clone());
    remove.amm = Some(AmmAction {
        token_id: token_id.clone(),
        shares: held,
        min_hkm: 1,
        min_token: 1,
        hkm_to_token: false,
        amount_in: 0,
        min_out: 0,
        amount_hkm: 0,
        amount_token: 0,
        min_shares: 0,
    });
    let message =
        Transaction::amm_remove_signing_message(&token_id, held, 1, 1, nonce);
    authorize(&mut remove, &message, &treasury);
    state.apply_transaction(&remove, 1).unwrap();

    let pool = state.pools.get(&token_id).unwrap();
    assert!(pool.total_shares > 0, "pool shares reached zero");
    assert!(pool.reserve_hkm > 0 && pool.reserve_token > 0);
}

// ===================================================================
// Tokens
// ===================================================================

/// Burning tokens you do not hold must fail.
#[test]
fn cannot_burn_tokens_you_do_not_hold() {
    let (mut state, treasury) = chain();
    let mut nonce = 1u64;
    let token_id = seed_pool(&mut state, &treasury, &mut nonce);
    let attacker = account(2);
    fund(&mut state, &treasury, &attacker.address, 100 * UNITS_PER_HKM, nonce);

    let supply_before = state.tokens.get(&token_id).unwrap().total_supply;
    let mut burn = Transaction::new(
        Some(attacker.address.clone()),
        attacker.address.clone(),
        1_000 * UNITS_PER_HKM,
        TransactionType::TokenBurn,
    );
    burn.nonce = 1;
    burn.public_key = Some(attacker.public_key.clone());
    burn.token = Some(hikmalayer::blockchain::transaction::TokenAction {
        token_id: token_id.clone(),
        ..Default::default()
    });
    let message =
        Transaction::token_burn_signing_message(&token_id, 1_000 * UNITS_PER_HKM, 1);
    authorize(&mut burn, &message, &attacker);

    assert!(state.apply_transaction(&burn, 1).is_err());
    assert_eq!(
        state.tokens.get(&token_id).unwrap().total_supply,
        supply_before
    );
}

/// Sending units of a token that does not exist must fail rather than
/// creating balances out of nothing.
#[test]
fn cannot_transfer_a_token_that_does_not_exist() {
    let (mut state, treasury) = chain();
    let ghost = "hkt0000000000000000000000000000000000000000";
    let recipient = account(2);

    let mut tx = Transaction::new(
        Some(treasury.address.clone()),
        recipient.address.clone(),
        1_000,
        TransactionType::TokenTransfer,
    );
    tx.nonce = 1;
    tx.public_key = Some(treasury.public_key.clone());
    tx.token = Some(hikmalayer::blockchain::transaction::TokenAction {
        token_id: ghost.to_string(),
        ..Default::default()
    });
    let message =
        Transaction::token_transfer_signing_message(ghost, &recipient.address, 1_000, 1);
    authorize(&mut tx, &message, &treasury);

    assert!(state.apply_transaction(&tx, 1).is_err());
    assert_eq!(state.token_balance_of(ghost, &recipient.address), 0);
}

// ===================================================================
// Vesting
// ===================================================================

/// Locked funds must not be spendable before they release, or the lockup is
/// decorative.
#[test]
fn vested_funds_are_not_spendable_before_the_cliff() {
    let (mut state, treasury) = chain();
    let beneficiary = account(2);
    let amount = 1_000 * UNITS_PER_HKM;

    let mut vest = Transaction::new(
        Some(treasury.address.clone()),
        beneficiary.address.clone(),
        amount,
        TransactionType::Vest,
    );
    vest.nonce = 1;
    vest.public_key = Some(treasury.public_key.clone());
    vest.vesting_cliff_blocks = Some(100);
    vest.vesting_duration_blocks = Some(400);
    let message = Transaction::vest_signing_message(
        &treasury.address,
        &beneficiary.address,
        amount,
        100,
        400,
        1,
    );
    authorize(&mut vest, &message, &treasury);
    state.apply_transaction(&vest, 10).unwrap();

    assert_eq!(state.balance_of(&beneficiary.address), 0);
    state.end_block(50, &treasury.address); // before the cliff
    assert_eq!(
        state.balance_of(&beneficiary.address),
        0,
        "funds released before the cliff"
    );

    // And the beneficiary cannot spend what has not released.
    let spend = signed_transfer(&beneficiary, &treasury.address, amount, 1);
    assert!(state.apply_transaction(&spend, 51).is_err());
}

// ===================================================================
// Determinism
// ===================================================================

/// Every node must compute the same state root from the same history, or the
/// network forks. This checks the root is sensitive to a single base unit.
#[test]
fn the_state_root_commits_to_every_base_unit() {
    let (state_a, treasury) = chain();
    let (mut state_b, _) = chain();
    assert_eq!(state_a.state_root(), state_b.state_root());

    let tx = signed_transfer(&treasury, &account(2).address, 1, 1);
    state_b.apply_transaction(&tx, 1).unwrap();
    assert_ne!(
        state_a.state_root(),
        state_b.state_root(),
        "a one-unit transfer left the state root unchanged"
    );
}

/// Applying the same transactions in the same order must produce the same
/// root, every time, on every node.
#[test]
fn identical_histories_produce_identical_roots() {
    let (mut a, treasury) = chain();
    let (mut b, _) = chain();
    let target = account(2);

    for nonce in 1..=10 {
        let tx = signed_transfer(&treasury, &target.address, nonce * UNITS_PER_HKM, nonce);
        a.apply_transaction(&tx, nonce).unwrap();
        b.apply_transaction(&tx, nonce).unwrap();
    }
    assert_eq!(a.state_root(), b.state_root());
}

// ===================================================================
// Cross-network replay
// ===================================================================

/// A transaction signed for one network must be inert on every other.
///
/// Addresses are derived from the key, so a user has the SAME address on a
/// testnet and on mainnet. Without a network binding, a transfer they signed
/// while testing is byte-identical to one spending their real funds, and
/// anyone who saw the testnet transaction can replay it against mainnet.
#[test]
fn a_transaction_signed_for_another_network_is_refused() {
    let (mut mainnet, treasury) = chain_named("hikmalayer-mainnet");
    let attacker = account(2);

    // The victim signs this on a testnet, where it is perfectly legitimate.
    let testnet_tx =
        signed_transfer_on("hikmalayer-testnet", &treasury, &attacker.address, 1_000 * UNITS_PER_HKM, 1);

    let before = mainnet.balance_of(&treasury.address);
    assert!(
        mainnet.apply_transaction(&testnet_tx, 1).is_err(),
        "a testnet transaction executed on mainnet"
    );
    assert_eq!(mainnet.balance_of(&treasury.address), before);
    assert_eq!(mainnet.balance_of(&attacker.address), 0);
}

/// Relabelling the transaction does not help: the network is inside the
/// signed message, so changing it invalidates the signature.
#[test]
fn relabelling_the_network_invalidates_the_signature() {
    let (mut mainnet, treasury) = chain_named("hikmalayer-mainnet");
    let attacker = account(2);

    let mut tx =
        signed_transfer_on("hikmalayer-testnet", &treasury, &attacker.address, 500 * UNITS_PER_HKM, 1);
    // Attacker rewrites the label to match the target chain.
    tx.chain_id = "hikmalayer-mainnet".to_string();

    assert!(tx.verify_authorization().is_err(), "relabelled transaction verified");
    assert!(mainnet.apply_transaction(&tx, 1).is_err());
    assert_eq!(mainnet.balance_of(&attacker.address), 0);
}

/// And the same transaction on its own network still works — the binding
/// must not break honest use.
#[test]
fn a_transaction_on_its_own_network_still_applies() {
    let (mut testnet, treasury) = chain_named("hikmalayer-testnet");
    let recipient = account(2);
    let tx =
        signed_transfer_on("hikmalayer-testnet", &treasury, &recipient.address, 42 * UNITS_PER_HKM, 1);
    testnet.apply_transaction(&tx, 1).unwrap();
    assert_eq!(testnet.balance_of(&recipient.address), 42 * UNITS_PER_HKM);
}
