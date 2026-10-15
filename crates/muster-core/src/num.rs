//! Whole numbers and fractions, for amounts set or measured as fractions
//! (dollars, CPUs, a share of memory) and shown as fractions (gigabytes).
//! Rust's `as` between them is well defined but quiet about what it loses;
//! these say it once, here.

/// `value` rounded to the nearest whole number. Below zero and NaN give 0,
/// and past `u64::MAX` gives `u64::MAX`, as `as` saturates.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "saturating, as documented"
)]
pub fn round_u64(value: f64) -> u64 {
    value.round() as u64
}

/// [`round_u64`] for `u32`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "saturating, as documented"
)]
pub fn round_u32(value: f64) -> u32 {
    value.round() as u32
}

/// `value` as a fraction, to scale or show: exact up to 2^53, the nearest
/// fraction past it.
#[expect(clippy::cast_precision_loss, reason = "nearest, as documented")]
pub fn fraction(value: u64) -> f64 {
    value as f64
}

/// [`fraction`] for a signed whole number, as TOML reads them.
#[expect(clippy::cast_precision_loss, reason = "nearest, as documented")]
pub fn fraction_signed(value: i64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_saturates_and_nan_is_zero() {
        assert_eq!(round_u64(2.5), 3);
        assert_eq!(round_u64(-4.0), 0);
        assert_eq!(round_u64(f64::NAN), 0);
        assert_eq!(round_u64(1e30), u64::MAX);
        assert_eq!(round_u32(1e12), u32::MAX);
        assert_eq!(round_u32(99.4), 99);
    }

    #[test]
    fn fractions_are_exact_while_they_can_be() {
        assert!((fraction(1 << 53) - 9_007_199_254_740_992.0).abs() < f64::EPSILON);
        assert!((fraction_signed(-3) + 3.0).abs() < f64::EPSILON);
    }
}
