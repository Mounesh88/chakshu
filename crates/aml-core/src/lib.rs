//! Shared types and helpers used by every Chakshu crate.

use anyhow::{Context, Result, bail};
use chrono::NaiveDateTime;

pub const PROJECT: &str = "Chakshu";

/// Approximate value of one unit of each currency in US dollars, early September 2022
/// (the period the IBM HI-Small data covers). Used only to put amounts on one scale for
/// rules and features; it is not an exchange-rate service.
const USD_PER_UNIT: &[(&str, f64)] = &[
    ("US Dollar", 1.0),
    ("Euro", 1.00),
    ("UK Pound", 1.15),
    ("Swiss Franc", 1.03),
    ("Canadian Dollar", 0.76),
    ("Australian Dollar", 0.68),
    ("Yuan", 0.144),
    ("Yen", 0.0070),
    ("Rupee", 0.0125),
    ("Ruble", 0.0165),
    ("Mexican Peso", 0.050),
    ("Brazil Real", 0.19),
    ("Shekel", 0.29),
    ("Saudi Riyal", 0.266),
    ("Bitcoin", 20_000.0),
];

/// Convert an amount to US dollars. Fails on a currency we have no rate for, so a new
/// currency in the data is noticed instead of silently counted as zero.
pub fn to_usd(amount: f64, currency: &str) -> Result<f64> {
    match USD_PER_UNIT.iter().find(|(name, _)| *name == currency) {
        Some((_, rate)) => Ok(amount * rate),
        None => bail!("no USD rate for currency {currency:?}; add it to USD_PER_UNIT in aml-core"),
    }
}

/// Parse the IBM data's timestamp format, e.g. "2022/09/01 00:20".
pub fn parse_ibm_timestamp(text: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text.trim(), "%Y/%m/%d %H:%M")
        .with_context(|| format!("timestamp {text:?} is not in the YYYY/MM/DD HH:MM format"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_currencies() {
        assert_eq!(to_usd(100.0, "US Dollar").unwrap(), 100.0);
        assert!((to_usd(1000.0, "Yuan").unwrap() - 144.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_unknown_currency() {
        assert!(to_usd(1.0, "Doubloon").is_err());
    }

    #[test]
    fn parses_ibm_timestamp() {
        let ts = parse_ibm_timestamp("2022/09/01 00:20").unwrap();
        assert_eq!(ts.to_string(), "2022-09-01 00:20:00");
        assert!(parse_ibm_timestamp("01-09-2022").is_err());
    }
}
