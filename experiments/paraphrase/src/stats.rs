//! The Wilson score interval (95%). Floats are fine here: this is a measurement report, not the
//! Tier-1 identity path, and every number is written to the record as a fixed 4-decimal string.

/// 97.5th percentile of the standard normal.
const Z: f64 = 1.959964;

/// A closed interval on a proportion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interval {
    pub lo: f64,
    pub hi: f64,
}

/// The 95% Wilson score interval for `k` successes in `n` trials; `None` when `n == 0` (no trials
/// is no estimate, never a 0% rate). Asymmetric by construction, and it stays inside [0, 1].
pub fn wilson(k: u32, n: u32) -> Option<Interval> {
    if n == 0 {
        return None;
    }
    let (k, n) = (f64::from(k.min(n)), f64::from(n));
    let p = k / n;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let centre = (p + z2 / (2.0 * n)) / denom;
    let half = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / denom;
    Some(Interval {
        lo: (centre - half).max(0.0),
        hi: (centre + half).min(1.0),
    })
}

/// A proportion as a fixed 4-decimal string, so the record never depends on float printing.
pub fn fixed4(x: f64) -> String {
    format!("{x:.4}")
}

/// `k/n` as a fixed 4-decimal string; `"n/a"` for no trials.
pub fn rate(k: u32, n: u32) -> String {
    if n == 0 {
        "n/a".to_string()
    } else {
        fixed4(f64::from(k) / f64::from(n))
    }
}
