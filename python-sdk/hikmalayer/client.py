"""
The Hikmalayer client: chain reads, signed writes, tokens and the AMM DEX.

Reads return int for amounts. Python's int is arbitrary precision, so a
10^17 balance survives JSON parsing intact — but the node is told to send
amounts as strings anyway (and accepts them either way), so nothing here
depends on the JSON parser's number handling.

    from hikmalayer import HikmalayerClient, LocalSigner, parse_hkm

    client = HikmalayerClient("http://127.0.0.1:3000", signer=LocalSigner.random())
    client.transfer(to="hkm…", amount=parse_hkm("10"))
"""

import json
import time
from typing import Any, Dict, List, Optional, Callable

import requests

from . import messages
from .signer import LocalSigner, is_valid_address, is_valid_token_id
from .units import to_base_units, apply_slippage, isqrt

# Response fields that carry on-chain amounts. Converted to int on read so a
# caller never accidentally does float arithmetic on a balance.
AMOUNT_FIELDS = {
    "amount", "balance", "base_fee", "burned", "fee", "initial_supply",
    "min_hkm", "min_out", "min_shares", "min_token", "amount_in", "amount_out",
    "releasable", "released", "reserve_hkm", "reserve_token", "shares",
    "stake", "staked", "total_shares", "total_supply", "vested",
}

MINIMUM_LIQUIDITY = 1_000  # matches ChainState::apply


class HikmalayerError(Exception):
    """A node-side rejection, surfaced rather than silently swallowed."""

    def __init__(self, message: str, *, status: Optional[int] = None,
                 body: Any = None, endpoint: Optional[str] = None):
        super().__init__(message)
        self.status = status
        self.body = body
        self.endpoint = endpoint


def _coerce_amounts(obj: Any) -> Any:
    """Walk a decoded response and turn known amount fields into int."""
    if isinstance(obj, dict):
        out = {}
        for key, value in obj.items():
            if key in AMOUNT_FIELDS and isinstance(value, (str, int)) \
                    and not isinstance(value, bool):
                try:
                    out[key] = int(value)
                    continue
                except (TypeError, ValueError):
                    pass
            out[key] = _coerce_amounts(value)
        return out
    if isinstance(obj, list):
        return [_coerce_amounts(item) for item in obj]
    return obj


class HikmalayerClient:
    """
    Client for a Hikmalayer node.

    :param url: node RPC base URL
    :param signer: a LocalSigner, for endpoints that require a signature
    :param admin_token: for admin-gated endpoints (faucet, governance)
    :param p2p_token: for P2P and snapshot endpoints
    :param timeout: per-request timeout in seconds
    """

    def __init__(self, url: str, *, signer: Optional[LocalSigner] = None,
                 admin_token: Optional[str] = None,
                 p2p_token: Optional[str] = None,
                 timeout: float = 30.0):
        self.url = url.rstrip("/")
        self.signer = signer
        self.admin_token = admin_token
        self.p2p_token = p2p_token
        self.timeout = timeout
        self.chain_id: Optional[str] = None
        self._session = requests.Session()

    # ---------------------------------------------------------------- plumbing

    def _request(self, method: str, path: str, *, body: Any = None,
                 admin: bool = False, p2p: bool = False) -> Any:
        headers = {"Content-Type": "application/json"}
        if admin:
            if not self.admin_token:
                raise HikmalayerError("admin_token is not configured",
                                      endpoint=path)
            headers["x-admin-token"] = self.admin_token
        if p2p:
            if not self.p2p_token:
                raise HikmalayerError("p2p_token is not configured",
                                      endpoint=path)
            headers["x-p2p-token"] = self.p2p_token

        response = self._session.request(
            method, f"{self.url}{path}",
            headers=headers,
            data=json.dumps(body) if body is not None else None,
            timeout=self.timeout,
        )

        try:
            payload = response.json()
        except ValueError:
            payload = response.text or None

        # The node returns 200 with {"status": "error"} for business-logic
        # rejections, so a status code alone is not enough to detect failure.
        if isinstance(payload, dict) and payload.get("status") == "error":
            raise HikmalayerError(payload.get("message", "request failed"),
                                  status=response.status_code,
                                  body=payload, endpoint=path)
        if not response.ok:
            raise HikmalayerError(f"HTTP {response.status_code}",
                                  status=response.status_code,
                                  body=payload, endpoint=path)

        return _coerce_amounts(payload)

    def _get(self, path: str, **kw) -> Any:
        return self._request("GET", path, **kw)

    def _post(self, path: str, body: Any = None, **kw) -> Any:
        return self._request("POST", path, body=body, **kw)

    def _require_signer(self) -> LocalSigner:
        if self.signer is None:
            raise HikmalayerError("This call needs a signer. Construct the "
                                  "client with signer=LocalSigner…")
        return self.signer

    def _network(self) -> str:
        """The chain id, fetched once and cached. Every signature is bound to it."""
        if self.chain_id is None:
            state = self.state()
            chain_id = state.get("chain_id")
            if not chain_id:
                raise HikmalayerError(
                    "The node did not report a chain_id, so a signature "
                    "cannot be bound to a network."
                )
            self.chain_id = chain_id
        return self.chain_id

    def _sign(self, domain: str) -> str:
        """Scope a domain to this network and sign it."""
        return self._require_signer().sign(messages.scoped(self._network(), domain))

    # ---------------------------------------------------------------- chain

    def stats(self) -> Dict[str, Any]:
        """Chain statistics: height, difficulty, finality, supply, base fee."""
        return self._get("/blockchain/stats")

    def state(self) -> Dict[str, Any]:
        """State root, height, supply, validator and account counts."""
        return self._get("/blockchain/state")

    def validate(self) -> Dict[str, Any]:
        """Ask the node to re-validate the whole chain."""
        return self._get("/blockchain/validate")

    def blocks(self, offset: int = 0, limit: int = 10) -> Any:
        return self._get(f"/blocks?offset={int(offset)}&limit={int(limit)}")

    def block(self, index: int) -> Any:
        return self._get(f"/blocks/{int(index)}")

    def pending(self) -> Any:
        """Transactions in the mempool, not yet mined."""
        return self._get("/transactions/pending")

    def fees(self) -> Dict[str, Any]:
        """The current dynamic base fee, in base units."""
        return self._get("/fees")

    def wait_for(self, predicate: Callable[[Dict[str, Any]], bool], *,
                 timeout_s: float = 30.0, interval_s: float = 0.25
                 ) -> Dict[str, Any]:
        """
        Poll /blockchain/stats until a predicate holds. Queued transactions
        only take effect when mined, so this is how you wait for one.

            before = client.stats()["total_blocks"]
            client.wait_for(lambda s: s["total_blocks"] > before)
        """
        deadline = time.monotonic() + timeout_s
        while True:
            snapshot = self.stats()
            if predicate(snapshot):
                return snapshot
            if time.monotonic() >= deadline:
                raise HikmalayerError(f"Timed out after {timeout_s}s waiting "
                                      "for the chain to advance")
            time.sleep(interval_s)

    # ---------------------------------------------------------------- explorer

    def overview(self) -> Dict[str, Any]:
        """Explorer summary: height, peers, validators, chain validity."""
        return self._get("/explorer/overview")

    def explorer_blocks(self, offset: int = 0, limit: int = 10) -> Any:
        return self._get(f"/explorer/blocks?offset={int(offset)}&limit={int(limit)}")

    def block_by_index(self, index: int) -> Any:
        return self._get(f"/explorer/blocks/index/{int(index)}")

    def block_by_hash(self, block_hash: str) -> Any:
        return self._get(f"/explorer/blocks/hash/{block_hash}")

    def search(self, query: str) -> Any:
        """Search by address (full history) or transaction id."""
        return self._get(f"/explorer/search/{query}")

    # ---------------------------------------------------------------- HKM

    def balance(self, address: Optional[str] = None) -> int:
        """On-chain HKM balance, in base units. Defaults to the signer."""
        who = address or self._require_signer().address
        return int(self._get(f"/tokens/balance/{who}")["balance"])

    def nonce(self, address: Optional[str] = None) -> Dict[str, Any]:
        """The next nonce for an account, and the current base fee."""
        who = address or self._require_signer().address
        return self._get(f"/tokens/nonce/{who}")

    def next_nonce(self, address: Optional[str] = None) -> int:
        """
        The next nonce to sign with, as a plain int.

        The node names this field `next_nonce`; `nonce` is accepted too so a
        rename on either side does not silently break signing.
        """
        payload = self.nonce(address)
        for field in ("next_nonce", "nonce"):
            if field in payload:
                return int(payload[field])
        raise HikmalayerError(
            f"The node did not return a nonce for this account: {payload!r}",
            endpoint="/tokens/nonce",
        )

    def transfer(self, *, to: str, amount, nonce: Optional[int] = None
                 ) -> Dict[str, Any]:
        """
        Send HKM. Amount is in base units — use parse_hkm("1.5") to convert.

            client.transfer(to="hkm…", amount=parse_hkm("10"))
        """
        if not is_valid_address(to):
            raise HikmalayerError(f"{to!r} is not a valid hkm address")
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(
            messages.transfer(signer.address, to, units, nonce))
        return self._post("/tokens/transfer", {
            "from": signer.address,
            "to": to,
            "amount": str(units),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def faucet(self, *, to: str, amount) -> Dict[str, Any]:
        """Admin only. Send HKM from the treasury (devnet/testnet)."""
        if not is_valid_address(to):
            raise HikmalayerError(f"{to!r} is not a valid hkm address")
        return self._post("/tokens/faucet",
                          {"to": to, "amount": str(to_base_units(amount))},
                          admin=True)

    # ---------------------------------------------------------------- vesting

    def vest(self, *, to: str, amount, cliff_blocks: int, duration_blocks: int,
             nonce: Optional[int] = None) -> Dict[str, Any]:
        """
        Lock HKM on a cliff + linear release schedule, enforced by consensus.
        Once mined the schedule cannot be altered.
        """
        if not is_valid_address(to):
            raise HikmalayerError(f"{to!r} is not a valid hkm address")
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.vest(
            signer.address, to, units, cliff_blocks, duration_blocks, nonce))
        return self._post("/tokens/vest", {
            "from": signer.address,
            "to": to,
            "amount": str(units),
            "cliff_blocks": int(cliff_blocks),
            "duration_blocks": int(duration_blocks),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def vesting(self, address: Optional[str] = None) -> Any:
        """Vesting schedules for an address."""
        who = address or self._require_signer().address
        return self._get(f"/vesting/{who}")

    # ---------------------------------------------------------------- HTS

    def assets(self) -> Any:
        """Every token in the registry."""
        return self._get("/assets")

    def asset(self, token_id: str) -> Any:
        """Token metadata: symbol, name, decimals, supply, creator."""
        return self._get(f"/assets/{token_id}")

    def asset_balance(self, token_id: str, address: Optional[str] = None) -> int:
        """Token balance for an address, in that token's own base units."""
        who = address or self._require_signer().address
        result = self._get(f"/assets/{token_id}/balance/{who}")
        return int(result["balance"]) if isinstance(result, dict) else int(result or 0)

    def create_asset(self, *, symbol: str, name: str, decimals: int,
                     initial_supply, nonce: Optional[int] = None
                     ) -> Dict[str, Any]:
        """
        Issue a native token. Supply is fixed at creation and can only ever
        be reduced by burning — no issuer can silently inflate it.
        """
        if len(symbol) > 12:
            raise HikmalayerError("A symbol is at most 12 characters")
        if len(name) > 64:
            raise HikmalayerError("A name is at most 64 characters")
        if not 0 <= int(decimals) <= 18:
            raise HikmalayerError("Decimals must be between 0 and 18")
        signer = self._require_signer()
        units = to_base_units(initial_supply)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.token_create(
            symbol, name, decimals, units, nonce))
        return self._post("/assets/create", {
            "creator": signer.address,
            "symbol": symbol,
            "name": name,
            "decimals": int(decimals),
            "initial_supply": str(units),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def transfer_asset(self, *, token_id: str, to: str, amount,
                       nonce: Optional[int] = None) -> Dict[str, Any]:
        """Move token units. Amounts are in the token's own base units."""
        if not is_valid_token_id(token_id):
            raise HikmalayerError(f"{token_id!r} is not a valid hkt token id")
        if not is_valid_address(to):
            raise HikmalayerError(f"{to!r} is not a valid hkm address")
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.token_transfer(
            token_id, to, units, nonce))
        return self._post("/assets/transfer", {
            "token_id": token_id,
            "from": signer.address,
            "to": to,
            "amount": str(units),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def burn_asset(self, *, token_id: str, amount,
                   nonce: Optional[int] = None) -> Dict[str, Any]:
        """Destroy token units permanently, reducing total supply."""
        if not is_valid_token_id(token_id):
            raise HikmalayerError(f"{token_id!r} is not a valid hkt token id")
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.token_burn(token_id, units, nonce))
        return self._post("/assets/burn", {
            "token_id": token_id,
            "from": signer.address,
            "amount": str(units),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    # ---------------------------------------------------------------- DEX

    def pools(self) -> Any:
        """Every active liquidity pool."""
        return self._get("/dex/pools")

    def pool(self, token_id: str) -> Any:
        """Reserves and total LP shares for one pool."""
        return self._get(f"/dex/pool/{token_id}")

    def lp_position(self, token_id: str, address: Optional[str] = None) -> Any:
        """An address's LP share position in a pool."""
        who = address or self._require_signer().address
        return self._get(f"/dex/position/{token_id}/{who}")

    def quote(self, token_id: str, *, hkm_to_token: bool, amount_in
              ) -> Dict[str, Any]:
        """
        The exact output the chain's constant-product maths would produce.
        Read-only — call this before every swap to derive a min_out bound.
        """
        direction = "hkm_to_token" if hkm_to_token else "token_to_hkm"
        units = to_base_units(amount_in)
        return self._get(f"/dex/quote/{token_id}/{direction}/{units}")

    def add_liquidity(self, *, token_id: str, amount_hkm, amount_token,
                      min_shares: Optional[int] = None,
                      slippage_bps: int = 50,
                      nonce: Optional[int] = None) -> Dict[str, Any]:
        """
        Deposit into a pool. The first depositor sets the price; later
        deposits must match the current ratio.

        min_shares defaults to the SDK's own prediction reduced by
        slippage_bps, so a deposit is never silently sandwiched.
        """
        if not is_valid_token_id(token_id):
            raise HikmalayerError(f"{token_id!r} is not a valid hkt token id")
        signer = self._require_signer()
        hkm_units = to_base_units(amount_hkm)
        token_units = to_base_units(amount_token)

        if min_shares is None:
            try:
                existing = self.pool(token_id)
            except HikmalayerError:
                existing = None
            predicted = self.quote_add_liquidity(existing, hkm_units, token_units)
            min_shares = apply_slippage(predicted["minted"], slippage_bps)

        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.amm_add(
            token_id, hkm_units, token_units, min_shares, nonce))
        return self._post("/dex/add", {
            "token_id": token_id,
            "provider": signer.address,
            "amount_hkm": str(hkm_units),
            "amount_token": str(token_units),
            "min_shares": str(int(min_shares)),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def remove_liquidity(self, *, token_id: str, shares,
                         min_hkm: Optional[int] = None,
                         min_token: Optional[int] = None,
                         slippage_bps: int = 50,
                         nonce: Optional[int] = None) -> Dict[str, Any]:
        """Burn LP shares for the proportional share of both reserves."""
        if not is_valid_token_id(token_id):
            raise HikmalayerError(f"{token_id!r} is not a valid hkt token id")
        signer = self._require_signer()
        share_units = to_base_units(shares)

        if min_hkm is None or min_token is None:
            predicted = self.quote_remove_liquidity(self.pool(token_id), share_units)
            if min_hkm is None:
                min_hkm = apply_slippage(predicted["amount_hkm"], slippage_bps)
            if min_token is None:
                min_token = apply_slippage(predicted["amount_token"], slippage_bps)

        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.amm_remove(
            token_id, share_units, min_hkm, min_token, nonce))
        return self._post("/dex/remove", {
            "token_id": token_id,
            "provider": signer.address,
            "shares": str(share_units),
            "min_hkm": str(int(min_hkm)),
            "min_token": str(int(min_token)),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    def swap(self, *, token_id: str, hkm_to_token: bool, amount_in,
             min_out: Optional[int] = None, slippage_bps: int = 50,
             nonce: Optional[int] = None) -> Dict[str, Any]:
        """
        Trade against a pool.

        min_out defaults to the on-chain quote reduced by slippage_bps, so a
        swap always carries a bound even when the caller forgets to set one.
        """
        if not is_valid_token_id(token_id):
            raise HikmalayerError(f"{token_id!r} is not a valid hkt token id")
        signer = self._require_signer()
        units = to_base_units(amount_in)

        if min_out is None:
            quoted = self.quote(token_id, hkm_to_token=hkm_to_token,
                                amount_in=units)
            min_out = apply_slippage(int(quoted["amount_out"]), slippage_bps)

        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.amm_swap(
            token_id, hkm_to_token, units, min_out, nonce))
        return self._post("/dex/swap", {
            "token_id": token_id,
            "trader": signer.address,
            "hkm_to_token": bool(hkm_to_token),
            "amount_in": str(units),
            "min_out": str(int(min_out)),
            "nonce": nonce,
            "public_key": signer.public_key,
            "signature": signature,
        })

    # ------------------------------------------------- AMM maths (offline)

    @staticmethod
    def quote_add_liquidity(pool: Optional[Dict[str, Any]],
                            amount_hkm: int, amount_token: int
                            ) -> Dict[str, int]:
        """
        Predict the shares a deposit will mint, matching ChainState::apply
        exactly. Pass pool=None for the first deposit into a new pool.
        """
        hkm = int(amount_hkm)
        token = int(amount_token)

        if not pool or int(pool.get("total_shares", 0)) == 0:
            minted = isqrt(hkm * token) - MINIMUM_LIQUIDITY
            return {
                "minted": max(minted, 0),
                "used_hkm": hkm,
                "used_token": token,
            }

        reserve_hkm = int(pool["reserve_hkm"])
        reserve_token = int(pool["reserve_token"])
        total_shares = int(pool["total_shares"])

        # The scarcer side binds the deposit.
        by_hkm = (hkm * total_shares) // reserve_hkm
        by_token = (token * total_shares) // reserve_token
        minted = min(by_hkm, by_token)

        return {
            "minted": minted,
            "used_hkm": (minted * reserve_hkm) // total_shares,
            "used_token": (minted * reserve_token) // total_shares,
        }

    @staticmethod
    def quote_remove_liquidity(pool: Dict[str, Any], shares: int
                               ) -> Dict[str, int]:
        """Predict the reserves a share burn returns."""
        total_shares = int(pool["total_shares"])
        if total_shares == 0:
            return {"amount_hkm": 0, "amount_token": 0}
        burn = int(shares)
        return {
            "amount_hkm": (burn * int(pool["reserve_hkm"])) // total_shares,
            "amount_token": (burn * int(pool["reserve_token"])) // total_shares,
        }

    # ---------------------------------------------------------------- staking

    def validators(self) -> Any:
        """Every registered validator and its stake."""
        return self._get("/staking/validators")

    def unbonding(self, address: Optional[str] = None) -> Any:
        """Unbonding status: amount, release height, slashing window."""
        who = address or self._require_signer().address
        return self._get(f"/staking/unbonding/{who}")

    def stake(self, *, amount, vrf_public_key: Optional[str] = None,
              nonce: Optional[int] = None) -> Dict[str, Any]:
        """
        Bond stake to become a validator. The minimum is 10,000 HKM.

        vrf_public_key comes from `hikma-wallet sign-stake`; it binds the
        VRF key used for leader election to this validator.
        """
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.stake(
            signer.address, units, nonce, vrf_public_key))
        body = {
            "address": signer.address,
            "amount": str(units),
            "public_key": signer.public_key,
            "nonce": nonce,
            "signature": signature,
        }
        if vrf_public_key:
            body["vrf_public_key"] = vrf_public_key
        return self._post("/staking/deposit", body)

    def withdraw(self, *, amount, nonce: Optional[int] = None) -> Dict[str, Any]:
        """Unbond stake. It stays slashable through the unbonding period."""
        signer = self._require_signer()
        units = to_base_units(amount)
        if nonce is None:
            nonce = self.next_nonce(signer.address)
        signature = self._sign(messages.withdraw(signer.address, units, nonce))
        return self._post("/staking/withdraw", {
            "address": signer.address,
            "amount": str(units),
            "nonce": nonce,
            "signature": signature,
        })

    # ---------------------------------------------------------------- mining

    def mine(self) -> Dict[str, Any]:
        """
        Ask the node to seal a block with its local validator key. A node
        that is not this slot's leader reports that, rather than erroring.
        """
        return self._post("/mine", {})

    def propose(self, validator: Optional[str] = None) -> Dict[str, Any]:
        """Get a PoW-mined unsigned block, for signing off-machine."""
        path = f"/mine/propose?validator={validator}" if validator else "/mine/propose"
        return self._post(path, {})

    def submit(self, *, block: Any, signature: str,
               validator_public_key: str) -> Dict[str, Any]:
        """Submit a block signed elsewhere (remote signer / HSM)."""
        return self._post("/mine/submit", {
            "block": block,
            "signature": signature,
            "validator_public_key": validator_public_key,
        })

    def difficulty(self) -> Any:
        return self._get("/mining/difficulty")

    # ---------------------------------------------------------------- governance

    def governance(self) -> Any:
        """Read the governance parameters."""
        return self._get("/governance/config")

    def set_governance(self, **params) -> Any:
        """Admin only. Update governance parameters."""
        return self._post("/governance/config", params, admin=True)

    # ---------------------------------------------------------------- slashing

    def submit_equivocation(self, *, block_a: Any, block_b: Any) -> Any:
        """
        Permissionless. Prove a validator signed two blocks at one height;
        the offender's stake is burned when the proof is mined.
        """
        return self._post("/slashing/equivocation",
                          {"block_a": block_a, "block_b": block_b})

    # ---------------------------------------------------------------- P2P

    def peers(self) -> Any:
        return self._get("/p2p/peers", p2p=True)

    def register_peer(self, address: str) -> Any:
        return self._post("/p2p/peers/register", {"address": address}, p2p=True)

    def peer_scores(self) -> Any:
        return self._get("/p2p/peers/scores", p2p=True)

    def checkpoint_bundle(self) -> Any:
        """A self-verifying bundle a fresh node can boot from."""
        return self._get("/checkpoint/bundle", p2p=True)
