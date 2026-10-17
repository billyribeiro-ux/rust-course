//! Pure deadline arithmetic before networking, scheduling or sleeps.
use std::time::Duration;
pub fn retry_budget(
    elapsed: Duration,
    total: Duration,
    backoff: Duration,
    per_attempt: Duration,
    attempts_used: usize,
    max_attempts: usize,
) -> Option<Duration> {
    if attempts_used >= max_attempts || per_attempt.is_zero() {
        return None;
    }
    let remaining = total.checked_sub(elapsed)?.checked_sub(backoff)?;
    if remaining.is_zero() {
        None
    } else {
        Some(remaining.min(per_attempt))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }
    #[test]
    fn caps_attempt_after_paying_backoff() {
        assert_eq!(
            retry_budget(ms(30), ms(100), ms(10), ms(80), 1, 3),
            Some(ms(60))
        );
        assert_eq!(
            retry_budget(ms(0), ms(100), ms(10), ms(20), 1, 3),
            Some(ms(20))
        );
    }
    #[test]
    fn exhausted_time_count_and_zero_timeout_have_no_attempt() {
        for elapsed in [ms(90), ms(100), ms(110)] {
            assert_eq!(retry_budget(elapsed, ms(100), ms(10), ms(20), 1, 3), None);
        }
        assert_eq!(retry_budget(ms(0), ms(100), ms(0), ms(20), 3, 3), None);
        assert_eq!(retry_budget(ms(0), ms(100), ms(0), ms(0), 0, 3), None);
        assert_eq!(retry_budget(ms(0), ms(100), ms(0), ms(20), 0, 0), None);
    }
    #[test]
    fn arithmetic_does_not_overflow_when_deadline_is_exceeded() {
        assert_eq!(
            retry_budget(Duration::MAX, Duration::MAX, ms(1), ms(20), 0, 1),
            None
        );
        assert_eq!(
            retry_budget(Duration::ZERO, Duration::MAX, Duration::ZERO, ms(20), 0, 1),
            Some(ms(20))
        );
    }
}
