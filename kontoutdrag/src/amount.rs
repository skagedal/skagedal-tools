//! Exact money, stored as thousandths of a krona.
//!
//! Bank statements carry at most three decimals and rounding errors in a
//! ledger are not acceptable, so amounts are integers rather than floats.

use std::fmt;
use std::str::FromStr;

/// A signed amount in thousandths of the account currency's major unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Amount(i64);

impl Amount {
    pub const ZERO: Amount = Amount(0);

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    /// The value as a float, for sorting and display only — never for sums.
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 1000.0
    }
}

impl std::ops::Add for Amount {
    type Output = Amount;
    fn add(self, other: Amount) -> Amount {
        Amount(self.0 + other.0)
    }
}

impl std::ops::AddAssign for Amount {
    fn add_assign(&mut self, other: Amount) {
        self.0 += other.0;
    }
}

impl std::iter::Sum for Amount {
    fn sum<I: Iterator<Item = Amount>>(iter: I) -> Amount {
        Amount(iter.map(|a| a.0).sum())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("not a valid amount: {0:?}")]
pub struct ParseAmountError(String);

impl FromStr for Amount {
    type Err = ParseAmountError;

    /// Accepts an optional sign, digits, an optional `.` or `,` decimal
    /// separator with up to three decimals, and spaces as thousands
    /// separators. More than three decimals is an error rather than a
    /// silent truncation.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let error = || ParseAmountError(text.to_string());
        let trimmed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        let (negative, digits) = match trimmed.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, trimmed.strip_prefix('+').unwrap_or(&trimmed)),
        };
        if digits.is_empty() {
            return Err(error());
        }

        let (whole, fraction) = match digits.split_once(['.', ',']) {
            Some((whole, fraction)) => (whole, fraction),
            None => (digits, ""),
        };
        if fraction.len() > 3 {
            return Err(error());
        }
        if whole.is_empty() && fraction.is_empty() {
            return Err(error());
        }
        if !whole.chars().all(|c| c.is_ascii_digit())
            || !fraction.chars().all(|c| c.is_ascii_digit())
        {
            return Err(error());
        }

        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| error())?
        };
        let scale = 10i64.pow(3 - fraction.len() as u32);
        let fraction: i64 = if fraction.is_empty() {
            0
        } else {
            fraction.parse().map_err(|_| error())?
        };

        let total = whole
            .checked_mul(1000)
            .and_then(|w| w.checked_add(fraction * scale))
            .ok_or_else(error)?;
        Ok(Amount(if negative { -total } else { total }))
    }
}

impl fmt::Display for Amount {
    /// Two decimals, which is what statements are read in. The third
    /// decimal is kept internally but has never been non-zero in practice.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}", self.as_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_three_decimal_form_seb_writes() {
        assert_eq!("-450.980".parse::<Amount>().unwrap(), Amount(-450_980));
        assert_eq!("10000.670".parse::<Amount>().unwrap(), Amount(10_000_670));
        assert_eq!("0.000".parse::<Amount>().unwrap(), Amount::ZERO);
    }

    #[test]
    fn parses_other_plausible_spellings() {
        assert_eq!("12".parse::<Amount>().unwrap(), Amount(12_000));
        assert_eq!("12,50".parse::<Amount>().unwrap(), Amount(12_500));
        assert_eq!("1 234.5".parse::<Amount>().unwrap(), Amount(1_234_500));
        assert_eq!("+7.25".parse::<Amount>().unwrap(), Amount(7_250));
    }

    #[test]
    fn rejects_rather_than_truncates() {
        assert!("1.2345".parse::<Amount>().is_err());
        assert!("".parse::<Amount>().is_err());
        assert!("-".parse::<Amount>().is_err());
        assert!("12kr".parse::<Amount>().is_err());
    }

    #[test]
    fn sums_exactly() {
        let total: Amount = ["-30.000", "-50.000", "-450.980"]
            .iter()
            .map(|s| s.parse::<Amount>().unwrap())
            .sum();
        assert_eq!(total, Amount(-530_980));
        assert_eq!(total.to_string(), "-530.98");
    }
}
