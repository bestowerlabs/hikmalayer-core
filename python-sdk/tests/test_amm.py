"""
The AMM maths, offline.

These mirror ChainState::apply. If the SDK's prediction and consensus ever
diverge, a user's slippage bound becomes meaningless — so the arithmetic is
asserted here rather than trusted.
"""

from hikmalayer import HikmalayerClient, isqrt

MINIMUM_LIQUIDITY = 1_000


class TestAddLiquidity:
    def test_first_deposit_mints_sqrt_minus_the_locked_minimum(self):
        hkm, token = 2_000_000, 1_000_000
        result = HikmalayerClient.quote_add_liquidity(None, hkm, token)
        assert result["minted"] == isqrt(hkm * token) - MINIMUM_LIQUIDITY

    def test_a_first_deposit_too_small_mints_nothing(self):
        result = HikmalayerClient.quote_add_liquidity(None, 10, 10)
        assert result["minted"] == 0

    def test_an_empty_pool_counts_as_a_first_deposit(self):
        empty = {"reserve_hkm": 0, "reserve_token": 0, "total_shares": 0}
        assert HikmalayerClient.quote_add_liquidity(empty, 2_000_000, 1_000_000) == \
            HikmalayerClient.quote_add_liquidity(None, 2_000_000, 1_000_000)

    def test_later_deposits_are_bound_by_the_scarcer_side(self):
        pool = {"reserve_hkm": 1_000_000, "reserve_token": 500_000,
                "total_shares": 700_000}
        # Matching ratio: both sides agree.
        balanced = HikmalayerClient.quote_add_liquidity(pool, 100_000, 50_000)
        assert balanced["minted"] == 70_000

        # Too much HKM: the token side binds.
        hkm_heavy = HikmalayerClient.quote_add_liquidity(pool, 200_000, 50_000)
        assert hkm_heavy["minted"] == 70_000

        # Too much token: the HKM side binds.
        token_heavy = HikmalayerClient.quote_add_liquidity(pool, 100_000, 200_000)
        assert token_heavy["minted"] == 70_000

    def test_reports_what_the_deposit_actually_uses(self):
        pool = {"reserve_hkm": 1_000_000, "reserve_token": 500_000,
                "total_shares": 700_000}
        result = HikmalayerClient.quote_add_liquidity(pool, 200_000, 50_000)
        # Only the balanced portion is used, not the full HKM offered.
        assert result["used_hkm"] <= 200_000
        assert result["used_token"] <= 50_000


class TestRemoveLiquidity:
    def test_returns_the_pro_rata_share_of_both_reserves(self):
        pool = {"reserve_hkm": 1_000_000, "reserve_token": 500_000,
                "total_shares": 1_000_000}
        result = HikmalayerClient.quote_remove_liquidity(pool, 100_000)
        assert result["amount_hkm"] == 100_000
        assert result["amount_token"] == 50_000

    def test_burning_nothing_withdraws_nothing(self):
        pool = {"reserve_hkm": 1_000_000, "reserve_token": 500_000,
                "total_shares": 1_000_000}
        result = HikmalayerClient.quote_remove_liquidity(pool, 0)
        assert result == {"amount_hkm": 0, "amount_token": 0}

    def test_an_empty_pool_returns_nothing(self):
        pool = {"reserve_hkm": 0, "reserve_token": 0, "total_shares": 0}
        assert HikmalayerClient.quote_remove_liquidity(pool, 100) == \
            {"amount_hkm": 0, "amount_token": 0}

    def test_exact_at_amounts_beyond_the_float_range(self):
        pool = {"reserve_hkm": 30_000_000_000_000_000,
                "reserve_token": 15_000_000_000_000_000,
                "total_shares": 20_000_000_000_000_000}
        result = HikmalayerClient.quote_remove_liquidity(pool, 10**16)
        assert result["amount_hkm"] == 15_000_000_000_000_000
        assert result["amount_token"] == 7_500_000_000_000_000
