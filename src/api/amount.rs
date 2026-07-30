//! Lossless deserialization of on-chain amounts.
//!
//! Every amount on Hikmalayer is an integer number of base units. HKM has 6
//! decimals and a 100-billion supply, so the largest legitimate amount is
//! 10^17 base units — comfortably past `Number.MAX_SAFE_INTEGER` (2^53 - 1 ≈
//! 9.007 × 10^15). A JavaScript client that puts such a value through
//! `Number` before serializing loses the low digits *silently*.
//!
//! That is not merely a display problem. Signatures cover the exact decimal
//! text of the amount, so a rounded value produces a request that no longer
//! matches what the user authorized: the node rejects a perfectly honest
//! transaction, and the reason is invisible to everyone involved.
//!
//! So amounts accept a JSON **string** as well as a number. Strings survive
//! any JSON stack intact, and `BigInt.toString()` is exact by construction.
//! Numbers keep working for small values and existing clients, but a
//! fractional or out-of-range number is refused rather than truncated.

use serde::de::{self, Deserializer, Unexpected, Visitor};
use std::fmt;

/// Deserialize a `u64` amount from either a JSON number or a decimal string.
pub fn deserialize<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(AmountVisitor)
}

/// Same, for fields that may be omitted entirely (`#[serde(default)]` style).
pub fn deserialize_default<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(OptionalAmountVisitor)
}

struct AmountVisitor;

impl<'de> Visitor<'de> for AmountVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an amount in base units, as an integer or a decimal string")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        Ok(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| E::custom("amount exceeds the maximum of u64"))
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<u64, E> {
        u64::try_from(value).map_err(|_| E::custom("amount is negative or exceeds u64"))
    }

    /// A float only arrives when the client already lost precision. Accept it
    /// when it is exactly integral and small enough to be trustworthy;
    /// otherwise refuse rather than silently round.
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<u64, E> {
        if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
            return Err(E::custom("amount must be a non-negative whole number"));
        }
        if value > 9_007_199_254_740_991.0 {
            return Err(E::custom(
                "amount exceeds 2^53-1 and cannot be represented exactly as a JSON number; send it as a decimal string",
            ));
        }
        Ok(value as u64)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<u64, E> {
        let text = value.trim();
        if text.is_empty() {
            return Err(E::custom("amount must not be empty"));
        }
        // Only plain digits: no sign, no exponent, no separators, no decimal
        // point. Anything else is a client bug we would rather surface.
        if !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(E::custom(
                "amount must be a whole number of base units, digits only",
            ));
        }
        text.parse::<u64>()
            .map_err(|_| E::custom("amount exceeds the maximum of u64"))
    }
}

struct OptionalAmountVisitor;

impl<'de> Visitor<'de> for OptionalAmountVisitor {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an optional amount in base units")
    }

    fn visit_none<E: de::Error>(self) -> Result<u64, E> {
        Ok(0)
    }

    fn visit_unit<E: de::Error>(self) -> Result<u64, E> {
        Ok(0)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<u64, D::Error> {
        deserializer.deserialize_any(AmountVisitor)
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<u64, E> {
        AmountVisitor.visit_u64(value)
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<u64, E> {
        AmountVisitor.visit_i64(value)
    }

    fn visit_u128<E: de::Error>(self, value: u128) -> Result<u64, E> {
        AmountVisitor.visit_u128(value)
    }

    fn visit_i128<E: de::Error>(self, value: i128) -> Result<u64, E> {
        AmountVisitor.visit_i128(value)
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<u64, E> {
        AmountVisitor.visit_f64(value)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<u64, E> {
        AmountVisitor.visit_str(value)
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "super::deserialize")]
        amount: u64,
        #[serde(default, deserialize_with = "super::deserialize_default")]
        min_out: u64,
    }

    fn parse(json: &str) -> Result<Holder, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn accepts_json_numbers() {
        let holder = parse(r#"{"amount": 1500000, "min_out": 42}"#).unwrap();
        assert_eq!(holder.amount, 1_500_000);
        assert_eq!(holder.min_out, 42);
    }

    #[test]
    fn accepts_decimal_strings() {
        let holder = parse(r#"{"amount": "1500000", "min_out": "42"}"#).unwrap();
        assert_eq!(holder.amount, 1_500_000);
        assert_eq!(holder.min_out, 42);
    }

    #[test]
    fn optional_amount_defaults_to_zero() {
        let holder = parse(r#"{"amount": "7"}"#).unwrap();
        assert_eq!(holder.min_out, 0);
    }

    /// The whole point: an amount past 2^53 survives as a string, exactly.
    #[test]
    fn preserves_amounts_beyond_the_float_safe_range() {
        // 100 billion HKM at 6 decimals — the entire supply, in base units.
        let holder = parse(r#"{"amount": "100000000000000000"}"#).unwrap();
        assert_eq!(holder.amount, 100_000_000_000_000_000);

        // One base unit less must be a genuinely different value.
        let holder = parse(r#"{"amount": "99999999999999999"}"#).unwrap();
        assert_eq!(holder.amount, 99_999_999_999_999_999);
    }

    #[test]
    fn accepts_u64_max_as_a_string() {
        let holder = parse(r#"{"amount": "18446744073709551615"}"#).unwrap();
        assert_eq!(holder.amount, u64::MAX);
    }

    #[test]
    fn rejects_values_beyond_u64() {
        assert!(parse(r#"{"amount": "18446744073709551616"}"#).is_err());
    }

    #[test]
    fn rejects_negative_and_fractional_input() {
        assert!(parse(r#"{"amount": -1}"#).is_err());
        assert!(parse(r#"{"amount": 1.5}"#).is_err());
        assert!(parse(r#"{"amount": "-1"}"#).is_err());
        assert!(parse(r#"{"amount": "1.5"}"#).is_err());
    }

    /// A float this large has already lost digits; refusing it is the only
    /// honest answer, since we cannot know which value was meant.
    #[test]
    fn rejects_imprecise_floats_instead_of_rounding() {
        assert!(parse(r#"{"amount": 1e17}"#).is_err());
    }

    #[test]
    fn rejects_junk_strings() {
        assert!(parse(r#"{"amount": ""}"#).is_err());
        assert!(parse(r#"{"amount": "1_000"}"#).is_err());
        assert!(parse(r#"{"amount": "1e6"}"#).is_err());
        assert!(parse(r#"{"amount": "0x10"}"#).is_err());
        assert!(parse(r#"{"amount": " 12 "}"#).is_ok()); // surrounding space is fine
    }
}
