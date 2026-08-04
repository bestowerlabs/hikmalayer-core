"""
Amount handling for Hikmalayer.

All on-chain amounts are integer BASE UNITS. Python's int is arbitrary
precision, so unlike JavaScript there is no 2^53 cliff — but the same
discipline applies: never use float for an amount, and never send a
float to the node.

    1 HKM = 1_000_000 base units (6 decimals)

Each native token (HTS) declares its own decimal count.
"""

from decimal import Decimal, InvalidOperation
from typing import Union

HKM_DECIMALS = 6
HKM_SCALE = 10 ** HKM_DECIMALS

Amount = Union[int, str, Decimal]


def parse_units(value: Union[str, int, Decimal], decimals: int = HKM_DECIMALS) -> int:
    """
    Parse a human-readable decimal amount into integer base units.

    >>> parse_units("1.5", 6)
    1500000
    >>> parse_units("10", 6)
    10000000
    >>> parse_units("0.000001", 6)
    1

    Raises ValueError on malformed input or excess precision.
    """
    text = str(value).strip()
    if not text:
        return 0

    if text.startswith("-"):
        raise ValueError("Amount must not be negative")

    try:
        dec = Decimal(text)
    except InvalidOperation as exc:
        raise ValueError(f"Amount must be a decimal number: {value!r}") from exc

    sign, digits, exponent = dec.as_tuple()
    if exponent < 0 and -exponent > decimals:
        raise ValueError(f"At most {decimals} decimal places for this asset")

    scaled = dec * (10 ** decimals)
    if scaled != scaled.to_integral_value():
        raise ValueError(f"At most {decimals} decimal places for this asset")

    return int(scaled)


def format_units(base_units: Amount, decimals: int = HKM_DECIMALS) -> str:
    """
    Format integer base units as a human-readable decimal string.

    >>> format_units(1500000, 6)
    '1.5'
    >>> format_units(1000000, 6)
    '1'
    >>> format_units(1, 6)
    '0.000001'
    """
    value = int(base_units)
    if decimals == 0:
        return str(value)

    negative = value < 0
    if negative:
        value = -value

    scale = 10 ** decimals
    whole = value // scale
    fraction = str(value % scale).rjust(decimals, "0").rstrip("0")

    text = f"{whole}.{fraction}" if fraction else str(whole)
    return f"-{text}" if negative else text


def parse_hkm(value: Union[str, int, Decimal]) -> int:
    """Shorthand: parse an HKM amount (6 decimals) into base units."""
    return parse_units(value, HKM_DECIMALS)


def format_hkm(base_units: Amount) -> str:
    """Shorthand: format HKM base units as a decimal string."""
    return format_units(base_units, HKM_DECIMALS)


def to_base_units(value: Amount) -> int:
    """
    Coerce an amount to an integer, refusing anything that has already
    lost precision. Mirrors the JS SDK's toBaseUnits.
    """
    if isinstance(value, bool):
        raise TypeError("A boolean is not an amount")
    if isinstance(value, int):
        return value
    if isinstance(value, Decimal):
        if value != value.to_integral_value():
            raise ValueError(f"{value} is not a whole number of base units")
        return int(value)
    if isinstance(value, str):
        text = value.strip()
        if not text.lstrip("-").isdigit():
            raise ValueError(f"{value!r} is not an integer amount")
        return int(text)
    if isinstance(value, float):
        raise TypeError(
            "A float amount has already lost precision. "
            "Pass an int or a decimal string instead."
        )
    raise TypeError(f"Cannot interpret {type(value).__name__} as an amount")


def apply_slippage(amount: int, tolerance_bps: int) -> int:
    """
    Apply a slippage tolerance in basis points to a quoted amount, producing
    a minimum-out bound. 50 bps = 0.5%.

    >>> apply_slippage(1000000, 50)
    995000
    """
    if not 0 <= tolerance_bps <= 10_000:
        raise ValueError("Tolerance must be between 0 and 10000 basis points")
    return (int(amount) * (10_000 - tolerance_bps)) // 10_000


def isqrt(n: int) -> int:
    """
    Integer square root, matching the chain's implementation exactly.
    Used for predicting LP shares on a first liquidity deposit.
    """
    if n < 0:
        raise ValueError("isqrt of a negative number")
    if n < 2:
        return n
    x = n
    y = (x + 1) // 2
    while y < x:
        x = y
        y = (x + n // x) // 2
    return x
