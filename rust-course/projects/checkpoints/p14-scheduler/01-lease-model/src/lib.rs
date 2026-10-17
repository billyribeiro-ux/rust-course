//! Deterministic, in-memory lease model. Time is an injected logical tick.
//! No persistence, OS worker, external effects, or clock guarantee is implemented.
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Queued,
    Running { token: u64, expires: u64 },
    Done,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Job {
    payload: String,
    state: State,
    attempts: u8,
    token: u64,
}
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct Queue {
    jobs: BTreeMap<String, Job>,
    effects: BTreeSet<String>,
}
impl Queue {
    pub fn enqueue(&mut self, id: &str, payload: &str) -> Result<bool, &'static str> {
        if id.is_empty() || id.len() > 64 || payload.len() > 256 {
            return Err("input budget");
        }
        if let Some(job) = self.jobs.get(id) {
            return if job.payload == payload {
                Ok(false)
            } else {
                Err("identity conflict")
            };
        }
        if self.jobs.len() >= 16 {
            return Err("retained job budget");
        }
        self.jobs.insert(
            id.into(),
            Job {
                payload: payload.into(),
                state: State::Queued,
                attempts: 0,
                token: 0,
            },
        );
        Ok(true)
    }
    pub fn claim(&mut self, id: &str, now: u64) -> Result<u64, &'static str> {
        let job = self.jobs.get_mut(id).ok_or("missing job")?;
        if !matches!(job.state, State::Queued)
            && !matches!(job.state,State::Running{expires,..}if expires<=now)
        {
            return Err("not eligible");
        }
        if job.attempts >= 3 {
            job.state = State::Failed;
            return Err("attempt budget exhausted");
        }
        let expires = now.checked_add(10).ok_or("clock overflow")?;
        let token = job.token.checked_add(1).ok_or("token overflow")?;
        job.token = token;
        job.attempts += 1;
        job.state = State::Running { token, expires };
        Ok(token)
    }
    pub fn complete(&mut self, id: &str, token: u64, now: u64) -> Result<bool, &'static str> {
        let job = self.jobs.get_mut(id).ok_or("missing job")?;
        if !matches!(job.state,State::Running{token:current,expires}if current==token&&now<expires)
        {
            return Err("stale or expired authority");
        }
        let inserted = self.effects.insert(id.into());
        job.state = State::Done;
        Ok(inserted)
    }
    pub fn cancel(&mut self, id: &str) -> Result<(), &'static str> {
        let job = self.jobs.get_mut(id).ok_or("missing job")?;
        if matches!(job.state, State::Done | State::Failed) {
            return Err("already terminal");
        }
        job.state = State::Cancelled;
        Ok(())
    }
    pub fn effects(&self) -> usize {
        self.effects.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expiry_redelivers_and_fences_old_worker() {
        let mut q = Queue::default();
        q.enqueue("a", "work").unwrap();
        let first = q.claim("a", 0).unwrap();
        assert!(q.claim("a", 9).is_err());
        let second = q.claim("a", 10).unwrap();
        assert_ne!(first, second);
        assert!(q.complete("a", first, 11).is_err());
        assert!(q.complete("a", second, 11).unwrap());
        assert!(q.complete("a", second, 12).is_err());
        assert_eq!(q.effects(), 1);
    }
    #[test]
    fn retry_conflict_and_capacity() {
        let mut q = Queue::default();
        for n in 0..16 {
            q.enqueue(&format!("j{n}"), "same").unwrap();
        }
        assert!(!q.enqueue("j0", "same").unwrap());
        assert!(q.enqueue("j0", "changed").is_err());
        assert!(q.enqueue("j17", "same").is_err());
    }
    #[test]
    fn cancellation_revokes_completion() {
        let mut q = Queue::default();
        q.enqueue("a", "work").unwrap();
        let token = q.claim("a", 0).unwrap();
        q.cancel("a").unwrap();
        assert!(q.complete("a", token, 1).is_err());
        assert_eq!(q.effects(), 0);
    }
    #[test]
    fn bad_clock_does_not_partially_claim() {
        let mut q = Queue::default();
        q.enqueue("a", "work").unwrap();
        let before = q.clone();
        assert!(q.claim("a", u64::MAX).is_err());
        assert_eq!(q, before);
        for now in [0, 10, 20] {
            q.claim("a", now).unwrap();
        }
        assert!(q.claim("a", 30).is_err());
        assert_eq!(q.jobs["a"].state, State::Failed);
    }
}
