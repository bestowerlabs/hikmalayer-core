"""Amount handling: no float ever touches an on-chain value."""

import pytest

from hikmalayer import (
    parse_units, format_units, parse_hkm, format_hkm,
    to_base_units, apply_slippage, isqrt, HKM_DECIMALS,
)

WHOLE_SUPPLY = 30_000_000_000_000_000  # 30B HKM in base units


class TestParseUnits:
    def test_whole_number(self):
        assert parse_units("1", 6) == 1_000_000

    def test_decimal(self):
        assert parse_units("1.5", 6) == 1_500_000

    def test_smallest_unit(self):
        assert parse_units("0.000001", 6) == 1

    def test_zero(self):
        assert parse_units("0", 6) == 0

    def test_empty_string_is_zero(self):
        assert parse_units("", 6) == 0

    def test_honours_a_tokens_own_decimals(self):
        assert parse_units("1.5", 8) == 150_000_000
        assert parse_units("1", 0) == 1

    def test_a_zero_decimal_token_refuses_a_fraction(self):
        # An indivisible asset cannot represent half a unit; rounding it
        # silently would hand the user an amount they did not sign.
        with pytest.raises(ValueError, match="decimal places"):
            parse_units("1.5", 0)

    def test_whole_supply_survives(self):
        assert parse_units("30000000000", 6) == WHOLE_SUPPLY

    def test_rejects_excess_precision(self):
        with pytest.raises(ValueError, match="decimal places"):
            parse_units("1.1234567", 6)

    def test_rejects_junk(self):
        with pytest.raises(ValueError):
            parse_units("abc", 6)

    def test_rejects_negative(self):
        with pytest.raises(ValueError):
            parse_units("-1", 6)


class TestFormatUnits:
    def test_whole(self):
        assert format_units(1_000_000, 6) == "1"

    def test_trailing_zeros_trimmed(self):
        assert format_units(1_500_000, 6) == "1.5"

    def test_smallest_unit(self):
        assert format_units(1, 6) == "0.000001"

    def test_zero(self):
        assert format_units(0, 6) == "0"

    def test_whole_supply(self):
        assert format_units(WHOLE_SUPPLY, 6) == "30000000000"


class TestRoundTrip:
    @pytest.mark.parametrize("value", [
        "0", "1", "10", "100", "0.5", "1.234567", "30000000000",
    ])
    def test_parse_then_format_is_identity(self, value):
        assert format_hkm(parse_hkm(value)) == value


class TestToBaseUnits:
    def test_accepts_int(self):
        assert to_base_units(1_000_000) == 1_000_000

    def test_accepts_digit_string(self):
        assert to_base_units("1000000") == 1_000_000

    def test_carries_amounts_past_2_53_exactly(self):
        # This is the case that silently broke in JavaScript.
        big = 2**53 + 1
        assert to_base_units(str(big)) == big
        assert to_base_units(big) == big

    def test_refuses_float(self):
        with pytest.raises(TypeError, match="lost precision"):
            to_base_units(1.5)

    def test_refuses_bool(self):
        with pytest.raises(TypeError):
            to_base_units(True)


class TestSlippage:
    def test_applies_basis_points(self):
        assert apply_slippage(1_000_000, 50) == 995_000  # 0.5%

    def test_zero_tolerance_is_exact(self):
        assert apply_slippage(1_000_000, 0) == 1_000_000

    def test_full_tolerance_is_no_floor(self):
        assert apply_slippage(1_000_000, 10_000) == 0

    def test_exact_beyond_float_range(self):
        big = 2**60
        assert apply_slippage(big, 50) == (big * 9950) // 10_000

    def test_rejects_out_of_range(self):
        with pytest.raises(ValueError):
            apply_slippage(100, 10_001)


class TestIsqrt:
    @pytest.mark.parametrize("n,expected", [
        (0, 0), (1, 1), (4, 2), (8, 2), (9, 3), (10**12, 10**6),
    ])
    def test_matches_expected(self, n, expected):
        assert isqrt(n) == expected

    def test_never_overestimates(self):
        for n in range(0, 1000):
            r = isqrt(n)
            assert r * r <= n < (r + 1) * (r + 1)

    def test_handles_amm_scale(self):
        # A 2:1 pool at 1e6/5e5 — the first-deposit share calculation.
        n = 2_000_000 * 1_000_000
        r = isqrt(n)
        assert r * r <= n < (r + 1) * (r + 1)
