"""
End-to-end: drive a live devnet with nothing but the SDK.

Needs a running node with a treasury key (so the faucet works) and an admin
token. `ops/devnet.sh` provides exactly that:

    ops/devnet.sh                    # in another terminal, foreground
    HIKMALAYER_ADMIN_TOKEN=devadmin pytest tests/integration/ -v

Skips itself when no node is reachable, so the offline suite stays green.

WHY THIS FILE IS SHAPED THE WAY IT IS
-------------------------------------
Transactions queue and execute when mined, so a test that submits one and
immediately asserts is racing the chain. Sealing a single block is not enough
either: the transaction may not make that block.

So nothing here waits for "a block". Every test waits for the *effect it
cares about* to become observable — see `await_change`. And every test that
spends, mints or burns gets its own funded account and its own token, so no
test can be perturbed by what another happened to do first.
"""

import os
import time
import pytest
import requests

from hikmalayer import (
    HikmalayerClient,
    HikmalayerError,
    LocalSigner,
    parse_units,
    parse_hkm,
)

URL = os.environ.get("HIKMALAYER_NODE", "http://127.0.0.1:3000")
ADMIN = os.environ.get("HIKMALAYER_ADMIN_TOKEN", "devadmin")

# Generous: a devnet seals every 5s, and a transaction may miss a block.
SETTLE_TIMEOUT_S = 60.0
POLL_INTERVAL_S = 0.5


def _reachable() -> bool:
    try:
        return requests.get(f"{URL}/blockchain/stats", timeout=3).ok
    except Exception:
        return False


pytestmark = pytest.mark.skipif(not _reachable(), reason=f"no node at {URL}")


# ---------------------------------------------------------------- waiting

class Settled:
    """
    Mines and waits until a caller-supplied condition holds.

    The unit of waiting is the *observable effect*, not elapsed blocks. A
    test says what it expects to become true and this drives the chain until
    it is — or fails with what it was still waiting for, which is far easier
    to debug than an assertion against stale state.
    """

    def __init__(self, admin: HikmalayerClient):
        self.admin = admin

    def _mine(self) -> None:
        try:
            self.admin.mine()
        except HikmalayerError:
            # Not this slot's leader. The devnet timer will seal it.
            pass

    def until(self, condition, what: str, timeout_s: float = SETTLE_TIMEOUT_S):
        """
        Drive the chain until `condition()` returns a truthy value.

        :param condition: called repeatedly; its return value is passed back
        :param what: described in the failure message if it never holds
        """
        deadline = time.monotonic() + timeout_s
        last_error = None

        while time.monotonic() < deadline:
            try:
                result = condition()
                if result:
                    return result
            except HikmalayerError as exc:
                # A read may legitimately 404 until the thing exists.
                last_error = exc

            self._mine()
            time.sleep(POLL_INTERVAL_S)

        detail = f" (last error: {last_error})" if last_error else ""
        raise AssertionError(
            f"Timed out after {timeout_s}s waiting for: {what}{detail}"
        )

    def balance(self, client: HikmalayerClient, expected: int):
        """Wait until an account's HKM balance reaches an exact value."""
        return self.until(
            lambda: client.balance() == expected,
            f"{client.signer.address} to hold {expected} base units",
        )

    def balance_increased_by(self, client: HikmalayerClient,
                             before: int, delta: int):
        """Wait until an account's balance has grown by exactly `delta`."""
        return self.until(
            lambda: client.balance() - before == delta,
            f"{client.signer.address} balance to grow by {delta}",
        )

    def asset_balance(self, client: HikmalayerClient, token_id: str,
                      expected: int):
        """Wait until a token balance reaches an exact value."""
        return self.until(
            lambda: client.asset_balance(token_id) == expected,
            f"token balance of {expected}",
        )

    def asset_supply(self, client: HikmalayerClient, token_id: str,
                     expected: int):
        """Wait until a token's total supply reaches an exact value."""
        return self.until(
            lambda: client.asset(token_id)["total_supply"] == expected,
            f"supply of {expected}",
        )

    def token_exists(self, client: HikmalayerClient, token_id: str):
        """Wait until a freshly issued token is readable on chain."""
        return self.until(
            lambda: client.asset(token_id),
            f"token {token_id} to exist",
        )

    def pool_exists(self, client: HikmalayerClient, token_id: str):
        """Wait until a pool has been created and has reserves."""
        return self.until(
            lambda: (lambda p: p if p and p.get("total_shares") else None)(
                client.pool(token_id)),
            f"a pool for {token_id}",
        )

    def pool_changed(self, client: HikmalayerClient, token_id: str,
                     before: dict):
        """Wait until a pool's reserves differ from a prior snapshot."""
        def changed():
            now = client.pool(token_id)
            if now and (now["reserve_hkm"] != before["reserve_hkm"]
                        or now["reserve_token"] != before["reserve_token"]):
                return now
            return None

        return self.until(changed, "the pool reserves to move")

    def nonce_above(self, client: HikmalayerClient, previous: int):
        """Wait until an account's nonce has advanced past a prior value."""
        return self.until(
            lambda: client.next_nonce() > previous,
            f"nonce to advance beyond {previous}",
        )


# ---------------------------------------------------------------- fixtures

@pytest.fixture(scope="module")
def admin() -> HikmalayerClient:
    return HikmalayerClient(URL, admin_token=ADMIN)


@pytest.fixture(scope="module")
def settled(admin) -> Settled:
    return Settled(admin)


@pytest.fixture(scope="module", autouse=True)
def chain_is_ready(admin, settled):
    """
    Refuse to run against a chain that has not finished bootstrapping.

    A node that is up but still has the pre-genesis supply will fail every
    test for reasons that have nothing to do with the SDK, so fail here with
    a message that says so.
    """
    state = settled.until(
        lambda: (lambda s: s if s["total_supply"] >= 30_000_000_000_000_000
                 else None)(admin.state()),
        "the devnet to finish bootstrapping (30B genesis supply)",
        timeout_s=90.0,
    )
    assert state["chain_id"], "the node did not report a chain_id"
    return state


@pytest.fixture
def funded(admin, settled):
    """
    An account funded for one test only.

    Module-scoped accounts accumulate the side effects of every test that
    ran before, so anything that spends gets a fresh one.
    """
    def _make(hkm: str = "50000") -> HikmalayerClient:
        client = HikmalayerClient(URL, signer=LocalSigner.random())
        amount = parse_hkm(hkm)
        admin.faucet(to=client.signer.address, amount=amount)
        settled.balance(client, amount)
        return client

    return _make


def _token_id_from(result: dict) -> str:
    """Pull the token id out of the node's creation response."""
    message = result.get("message", "")
    token_id = None
    for part in message.replace(",", " ").split():
        if part.startswith("token_id="):
            token_id = part.split("=", 1)[1]
        elif part.startswith("hkt"):
            token_id = part
    assert token_id, f"could not find a token id in {message!r}"
    return token_id


@pytest.fixture
def issued_token(funded, settled):
    """A token issued for one test, with its issuer."""
    counter = {"n": 0}

    def _make(supply: str = "1000000", decimals: int = 6) -> dict:
        counter["n"] += 1
        issuer = funded()
        units = parse_units(supply, decimals)

        result = issuer.create_asset(
            symbol=f"PYT{counter['n']}",
            name=f"Python SDK Test Asset {counter['n']}",
            decimals=decimals,
            initial_supply=units,
        )
        token_id = _token_id_from(result)
        settled.token_exists(issuer, token_id)
        settled.asset_balance(issuer, token_id, units)

        return {"id": token_id, "supply": units,
                "decimals": decimals, "issuer": issuer}

    return _make


@pytest.fixture
def seeded_pool(issued_token, settled):
    """A token with its own freshly seeded pool, owned by one test."""
    def _make(hkm: str = "10000", tokens: str = "50000") -> dict:
        token = issued_token()
        issuer = token["issuer"]

        amount_hkm = parse_hkm(hkm)
        amount_token = parse_units(tokens, token["decimals"])

        issuer.add_liquidity(token_id=token["id"],
                             amount_hkm=amount_hkm,
                             amount_token=amount_token)
        pool = settled.pool_exists(issuer, token["id"])

        return {**token, "seed_hkm": amount_hkm,
                "seed_token": amount_token, "pool": pool}

    return _make


# ---------------------------------------------------------------- reads

class TestChainReads:
    """Reads only — safe to share the module-scoped client."""

    def test_state_amounts_are_exact_ints(self, admin):
        state = admin.state()
        assert isinstance(state["total_supply"], int)
        # Beyond 2^53, and still exact — the case that broke in JavaScript.
        assert state["total_supply"] >= 30_000_000_000_000_000
        assert state["total_supply"] > 2**53

    def test_stats_reports_a_healthy_chain(self, admin):
        stats = admin.stats()
        assert stats["is_valid"] is True
        assert stats["total_blocks"] > 0
        assert isinstance(stats["base_fee"], int)

    def test_the_node_reports_a_chain_id(self, admin):
        assert admin.state()["chain_id"]

    def test_explorer_overview(self, admin):
        overview = admin.overview()
        assert overview["chain_valid"] is True
        assert overview["total_blocks"] > 0

    def test_blocks_paginate(self, admin):
        blocks = admin.explorer_blocks(0, 3)
        assert blocks["blocks"]
        assert len(blocks["blocks"]) <= 3


# ---------------------------------------------------------------- HKM

class TestNativeTransfers:
    def test_the_faucet_funds_an_account(self, funded):
        # `funded` waits for the exact balance, so arriving here is the pass.
        account = funded("1000")
        assert account.balance() == parse_hkm("1000")

    def test_a_transfer_moves_the_exact_amount(self, funded, settled):
        sender = funded()
        recipient = LocalSigner.random()
        watcher = HikmalayerClient(URL, signer=recipient)

        amount = parse_hkm("250")
        sender.transfer(to=recipient.address, amount=amount)
        settled.balance(watcher, amount)

        assert watcher.balance() == amount

    def test_nonces_advance_without_being_managed_by_hand(self, funded, settled):
        sender = funded()
        before = sender.next_nonce()

        sender.transfer(to=LocalSigner.random().address, amount=parse_hkm("1"))
        settled.nonce_above(sender, before)

        assert sender.next_nonce() > before

    def test_a_transfer_the_sender_cannot_afford_is_refused(self, funded):
        sender = funded("10")
        with pytest.raises(HikmalayerError):
            sender.transfer(to=LocalSigner.random().address,
                            amount=parse_hkm("999999"))

    def test_a_malformed_recipient_is_refused_before_signing(self, funded):
        sender = funded("10")
        with pytest.raises(HikmalayerError, match="not a valid hkm address"):
            sender.transfer(to="hkm-typo", amount=parse_hkm("1"))

    def test_an_amount_past_2_53_survives_the_round_trip(self, admin, settled):
        # The exact value whose low digits vanished in the JS SDK.
        amount = 2**53 + 1
        assert admin.state()["total_supply"] > amount, \
            "devnet treasury cannot cover this — restart the devnet"

        recipient = LocalSigner.random()
        admin.faucet(to=recipient.address, amount=amount)

        client = HikmalayerClient(URL, signer=recipient)
        settled.balance(client, amount)
        assert client.balance() == amount


# ---------------------------------------------------------------- HTS

class TestNativeTokens:
    def test_the_whole_supply_is_minted_to_the_issuer(self, issued_token):
        token = issued_token()
        assert token["issuer"].asset_balance(token["id"]) == token["supply"]

    def test_metadata_round_trips(self, issued_token):
        token = issued_token()
        asset = token["issuer"].asset(token["id"])

        assert asset["symbol"].startswith("PYT")
        assert asset["decimals"] == 6
        assert asset["total_supply"] == token["supply"]

    def test_a_token_appears_in_the_registry(self, issued_token):
        token = issued_token()
        ids = {a.get("token_id") or a.get("id")
               for a in token["issuer"].assets()}
        assert token["id"] in ids

    def test_transfer_moves_token_units(self, issued_token, funded, settled):
        token = issued_token()
        recipient = funded("10")

        amount = parse_units("2500", 6)
        token["issuer"].transfer_asset(token_id=token["id"],
                                       to=recipient.signer.address,
                                       amount=amount)
        settled.asset_balance(recipient, token["id"], amount)

        assert recipient.asset_balance(token["id"]) == amount

    def test_burning_reduces_supply(self, issued_token, settled):
        token = issued_token()
        burn = parse_units("1000", 6)
        expected = token["supply"] - burn

        token["issuer"].burn_asset(token_id=token["id"], amount=burn)
        settled.asset_supply(token["issuer"], token["id"], expected)

        assert token["issuer"].asset(token["id"])["total_supply"] == expected

    def test_burning_more_than_you_hold_is_refused(self, issued_token):
        token = issued_token()
        with pytest.raises(HikmalayerError):
            token["issuer"].burn_asset(token_id=token["id"],
                                       amount=token["supply"] * 2)

    def test_an_unknown_token_is_refused(self, funded):
        client = funded("10")
        with pytest.raises(HikmalayerError):
            client.transfer_asset(token_id="hkt" + "0" * 40,
                                  to=client.signer.address, amount=1)

    def test_a_malformed_token_id_is_refused_before_signing(self, funded):
        client = funded("10")
        with pytest.raises(HikmalayerError, match="not a valid hkt token id"):
            client.transfer_asset(token_id="not-a-token",
                                  to=client.signer.address, amount=1)


# ---------------------------------------------------------------- DEX

class TestAmmDex:
    """
    Every test seeds its own pool and funds its own trader. These are
    assertions about exact arithmetic; a pool another test has traded
    against would break them for reasons unrelated to the SDK.
    """

    def test_seeding_a_pool_mints_the_predicted_shares(self, issued_token,
                                                        settled):
        token = issued_token()
        issuer = token["issuer"]

        hkm = parse_hkm("10000")
        tokens = parse_units("50000", 6)
        predicted = HikmalayerClient.quote_add_liquidity(None, hkm, tokens)

        issuer.add_liquidity(token_id=token["id"],
                             amount_hkm=hkm, amount_token=tokens)
        pool = settled.pool_exists(issuer, token["id"])

        assert issuer.lp_position(token["id"])["shares"] == predicted["minted"]
        assert pool["reserve_hkm"] == hkm
        assert pool["reserve_token"] == tokens

    def test_a_swap_delivers_exactly_the_quoted_amount(self, seeded_pool,
                                                        funded, settled):
        token = seeded_pool()
        buyer = funded()

        spend = parse_hkm("100")
        quote = buyer.quote(token["id"], hkm_to_token=True, amount_in=spend)

        buyer.swap(token_id=token["id"], hkm_to_token=True, amount_in=spend)
        settled.asset_balance(buyer, token["id"], quote["amount_out"])

        assert buyer.asset_balance(token["id"]) == quote["amount_out"]

    def test_the_second_buyer_of_the_same_size_gets_less(self, seeded_pool,
                                                          funded, settled):
        token = seeded_pool()
        buyer = funded()
        spend = parse_hkm("100")

        first = buyer.quote(token["id"], hkm_to_token=True, amount_in=spend)
        buyer.swap(token_id=token["id"], hkm_to_token=True, amount_in=spend)
        settled.asset_balance(buyer, token["id"], first["amount_out"])

        # The curve has moved against the next buyer of the same size.
        second = buyer.quote(token["id"], hkm_to_token=True, amount_in=spend)
        assert second["amount_out"] < first["amount_out"]

    def test_the_constant_product_grows_by_the_fee(self, seeded_pool,
                                                    funded, settled):
        token = seeded_pool()
        buyer = funded()

        before = buyer.pool(token["id"])
        k_before = before["reserve_hkm"] * before["reserve_token"]

        buyer.swap(token_id=token["id"], hkm_to_token=True,
                   amount_in=parse_hkm("50"))
        after = settled.pool_changed(buyer, token["id"], before)

        # The 0.30% fee stays in the pool, so k only ever grows.
        assert after["reserve_hkm"] * after["reserve_token"] > k_before

    def test_an_unsatisfiable_slippage_bound_is_refused(self, seeded_pool,
                                                        funded):
        token = seeded_pool()
        buyer = funded()

        spend = parse_hkm("100")
        quote = buyer.quote(token["id"], hkm_to_token=True, amount_in=spend)

        with pytest.raises(HikmalayerError):
            buyer.swap(token_id=token["id"], hkm_to_token=True,
                       amount_in=spend, min_out=quote["amount_out"] * 2)

    def test_withdrawing_matches_the_predicted_amounts(self, seeded_pool,
                                                        settled):
        token = seeded_pool()
        issuer = token["issuer"]

        # Read and predict together: nothing else touches this pool, so the
        # prediction cannot go stale between here and the withdrawal.
        before = issuer.pool(token["id"])
        shares = issuer.lp_position(token["id"])["shares"] // 2
        predicted = HikmalayerClient.quote_remove_liquidity(before, shares)

        issuer.remove_liquidity(token_id=token["id"], shares=shares)
        after = settled.pool_changed(issuer, token["id"], before)

        assert before["reserve_hkm"] - after["reserve_hkm"] == \
            predicted["amount_hkm"]
        assert before["reserve_token"] - after["reserve_token"] == \
            predicted["amount_token"]

    def test_a_pool_that_does_not_exist_quotes_nothing(self, admin):
        # Finding nothing is an answer, not a failure.
        missing = "hkt" + "0" * 40
        assert admin.quote(missing, hkm_to_token=True, amount_in=1) is None
        assert admin.pool(missing) is None


# ---------------------------------------------------------------- vesting

class TestVesting:
    def test_nothing_releases_before_the_cliff(self, funded, settled):
        granter = funded()
        beneficiary = LocalSigner.random()
        watcher = HikmalayerClient(URL, signer=beneficiary)

        # A cliff far enough out that it cannot pass during the test.
        granter.vest(to=beneficiary.address, amount=parse_hkm("1000"),
                     cliff_blocks=1_000_000, duration_blocks=4_000_000)

        # Wait for the lock to be recorded, then confirm nothing is spendable.
        settled.until(
            lambda: granter.vesting(beneficiary.address),
            "the vesting schedule to be recorded",
        )
        assert watcher.balance() == 0

    def test_the_schedule_is_visible_on_chain(self, funded, settled):
        granter = funded()
        beneficiary = LocalSigner.random()

        granter.vest(to=beneficiary.address, amount=parse_hkm("500"),
                     cliff_blocks=1_000_000, duration_blocks=4_000_000)

        schedules = settled.until(
            lambda: granter.vesting(beneficiary.address),
            "the vesting schedule to be recorded",
        )
        assert schedules


# ---------------------------------------------------------------- errors

class TestErrorHandling:
    def test_node_rejections_surface_as_errors(self, funded):
        sender = funded("10")
        with pytest.raises(HikmalayerError) as excinfo:
            sender.transfer(to=LocalSigner.random().address,
                            amount=parse_hkm("999999"))

        assert excinfo.value.endpoint == "/tokens/transfer"
        assert excinfo.value.body is not None

    def test_an_admin_endpoint_without_a_token_is_refused(self):
        client = HikmalayerClient(URL)
        with pytest.raises(HikmalayerError, match="admin_token"):
            client.faucet(to=LocalSigner.random().address, amount=1)

    def test_a_signing_call_without_a_signer_is_refused(self, admin):
        with pytest.raises(HikmalayerError, match="needs a signer"):
            admin.transfer(to=LocalSigner.random().address, amount=1)

    def test_a_lookup_that_finds_nothing_returns_none(self, admin):
        assert admin.pool("hkt" + "0" * 40) is None
