//! Detection of fabricated metrics/measurements.

/// Checks whether two values from DIFFERENT execution profiles are suspiciously
/// identical. `tolerance` is the relative difference threshold.
pub fn suspiciously_identical(a: f64, b: f64, tolerance: f64) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return false;
    }
    let scale = a.abs().max(b.abs());
    if scale == 0.0 {
        return false;
    }
    (a - b).abs() / scale < tolerance
}

pub use suspiciously_identical as podejrzanie_identyczna;

/// Default profile comparison tolerance: 0.1%.
pub const PROFILE_TOLERANCE: f64 = 0.001;
pub const TOLERANCJA_PROFILI: f64 = PROFILE_TOLERANCE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_65_caught() {
        assert!(
            suspiciously_identical(65.0, 65.0, PROFILE_TOLERANCE),
            "identical throughput across profiles is an alarm"
        );
    }

    #[test]
    fn real_measurements_pass() {
        let vllm = 40.0 / 0.910;
        let ollama = 26.0 / 7.798;
        assert!(!suspiciously_identical(vllm, ollama, PROFILE_TOLERANCE));
    }

    #[test]
    fn close_but_not_identical_does_not_alarm() {
        assert!(!suspiciously_identical(64.0, 65.0, PROFILE_TOLERANCE));
    }

    #[test]
    fn zeros_do_not_alarm() {
        assert!(!suspiciously_identical(0.0, 0.0, PROFILE_TOLERANCE));
    }
}
