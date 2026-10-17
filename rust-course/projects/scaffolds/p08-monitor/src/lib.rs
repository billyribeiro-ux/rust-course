use std::time::Duration;
/// A retry may start only if attempts remain and its backoff fits before the deadline.
/// Return the remaining attempt budget capped by per_attempt after paying backoff.
pub fn retry_budget(_elapsed: Duration, _total: Duration, _backoff: Duration, _per_attempt: Duration, _attempts_used: usize, _max_attempts: usize) -> Option<Duration> {
    todo!("Learner: checked subtraction; reject exhausted count and zero remaining time")
}
#[cfg(test)] mod acceptance {
    use super::*;
    fn ms(n: u64) -> Duration { Duration::from_millis(n) }
    #[test] fn bounds_next_attempt() { assert_eq!(retry_budget(ms(30), ms(100), ms(10), ms(80), 1, 3), Some(ms(60))); }
    #[test] fn respects_per_attempt_cap() { assert_eq!(retry_budget(ms(0), ms(100), ms(10), ms(20), 1, 3), Some(ms(20))); }
    #[test] fn no_time_or_attempts_means_no_retry() { for elapsed in [ms(90), ms(100), ms(110)] { assert_eq!(retry_budget(elapsed, ms(100), ms(10), ms(20), 1, 3), None); } assert_eq!(retry_budget(ms(0), ms(100), ms(0), ms(20), 3, 3), None); }
}
