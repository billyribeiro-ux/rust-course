//! Persisted, bounded replay jobs inside the single-writer modular application.
//! Every step journals the job cursor and tick effect together; no external effects.
use crate::{Engine, Event, Store, Tick};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplaySpec {
    pub job_id: String,
    pub ticks: Vec<Tick>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}
#[derive(Debug, Clone)]
pub(crate) struct ReplayJob {
    pub spec: ReplaySpec,
    pub next: usize,
    pub status: ReplayStatus,
    pub error: Option<String>,
}
impl ReplayJob {
    pub fn active(&self) -> bool {
        matches!(self.status, ReplayStatus::Queued | ReplayStatus::Running)
    }
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct JobView {
    pub job_id: String,
    pub next: usize,
    pub total: usize,
    pub status: ReplayStatus,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChartPoint {
    pub event_position: u64,
    pub symbol: String,
    pub price_cents: i64,
    pub sequence: u64,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct History {
    pub points: Vec<ChartPoint>,
    pub oldest_position: Option<u64>,
    pub latest_position: Option<u64>,
}
impl Engine {
    pub fn jobs(&self) -> Vec<JobView> {
        self.jobs
            .values()
            .map(|j| JobView {
                job_id: j.spec.job_id.clone(),
                next: j.next,
                total: j.spec.ticks.len(),
                status: j.status.clone(),
                error: j.error.clone(),
            })
            .collect()
    }
    pub fn history(&self, after: u64, limit: usize) -> Result<History, &'static str> {
        if !(1..=256).contains(&limit) {
            return Err("history limit outside 1..=256");
        }
        let oldest = self.history.front().map(|p| p.event_position);
        if after != 0 && oldest.is_some_and(|p| after < p.saturating_sub(1)) {
            return Err("history cursor expired; restart from after=0");
        }
        Ok(History {
            points: self
                .history
                .iter()
                .filter(|p| p.event_position > after)
                .take(limit)
                .cloned()
                .collect(),
            oldest_position: oldest,
            latest_position: self.history.back().map(|p| p.event_position),
        })
    }
}
impl Store {
    /// Executes one bounded, durable cursor increment. A process restart resumes the cursor.
    /// No retry timer is needed for CPU-only deterministic work; invalid domain input becomes failed.
    pub fn advance_replay(&mut self) -> Result<bool, crate::CommitError> {
        let Some(job) = self.engine.jobs.values().find(|j| j.active()) else {
            return Ok(false);
        };
        let event = Event::ReplayStep {
            job_id: job.spec.job_id.clone(),
            index: job.next,
        };
        let event = match self.engine.validate(&event) {
            Ok(_) => event,
            Err(reason) => Event::FailReplay {
                job_id: job.spec.job_id.clone(),
                reason: reason.to_owned(),
            },
        };
        self.commit(event)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "course-replay-{label}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    fn spec(id: &str) -> ReplaySpec {
        ReplaySpec {
            job_id: id.into(),
            ticks: (1..=3)
                .map(|n| Tick {
                    event_id: format!("{id}-{n}"),
                    symbol: "RUST".into(),
                    price_cents: 100 + n as i64,
                    sequence: n,
                })
                .collect(),
        }
    }
    #[test]
    fn job_restart_atomic_cursor_duplicate_and_cancellation() {
        let path = path("restart");
        let input = spec("job1");
        {
            let mut store = Store::open(&path).unwrap();
            assert!(store.commit(Event::ScheduleReplay(input.clone())).unwrap());
            assert!(!store.commit(Event::ScheduleReplay(input.clone())).unwrap());
            assert!(store.advance_replay().unwrap());
            assert_eq!(store.engine.jobs()[0].next, 1);
        }
        {
            let mut store = Store::open(&path).unwrap();
            assert_eq!(store.engine.jobs()[0].next, 1);
            assert_eq!(store.engine.history(0, 10).unwrap().points.len(), 1);
            assert!(store.advance_replay().unwrap());
            store
                .commit(Event::CancelReplay {
                    job_id: "job1".into(),
                })
                .unwrap();
            assert!(!store.advance_replay().unwrap());
            assert_eq!(store.engine.jobs()[0].status, ReplayStatus::Cancelled);
            assert!(
                store
                    .commit(Event::ReplayStep {
                        job_id: "job1".into(),
                        index: 2
                    })
                    .is_err()
            );
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.engine.jobs()[0].next, 2);
        assert_eq!(store.engine.jobs()[0].status, ReplayStatus::Cancelled);
        drop(store);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn completed_history_and_invalid_job_fail_deterministically() {
        let path = path("complete");
        let mut s = Store::open(&path).unwrap();
        s.commit(Event::ScheduleReplay(spec("a"))).unwrap();
        while s.advance_replay().unwrap() {}
        assert_eq!(s.engine.jobs()[0].status, ReplayStatus::Completed);
        let history = s.engine.history(0, 2).unwrap();
        assert_eq!(history.points.len(), 2);
        let last = history.points.last().unwrap().event_position;
        assert_eq!(s.engine.history(last, 2).unwrap().points.len(), 1);
        s.commit(Event::ScheduleReplay(spec("b"))).unwrap();
        s.advance_replay().unwrap();
        assert_eq!(s.engine.jobs()[1].status, ReplayStatus::Failed);
        assert_eq!(s.engine.history(0, 10).unwrap().points.len(), 3);
        drop(s);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn legacy_migration_and_position_corruption() {
        let path = path("migration");
        std::fs::write(&path, include_bytes!("../fixtures/legacy-v1.jsonl")).unwrap();
        {
            let mut s = Store::open(&path).unwrap();
            assert_eq!(s.engine.snapshot().cash_cents, 999750);
            s.commit(Event::Watchlist(vec!["RUST".into(), "LAB".into()]))
                .unwrap();
        }
        let data = std::fs::read_to_string(&path).unwrap();
        assert!(data.contains("\"version\":2,\"position\":3"));
        assert!(Store::open(&path).is_ok());
        std::fs::write(&path, data.replace("\"position\":3", "\"position\":99")).unwrap();
        assert!(Store::open(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn history_memory_and_cursor_are_bounded() {
        let mut e = Engine::default();
        for n in 1..=1100 {
            e.apply(Event::Tick(Tick {
                event_id: format!("t-{n}"),
                symbol: "RUST".into(),
                price_cents: 100,
                sequence: n,
            }))
            .unwrap();
        }
        assert_eq!(e.history.len(), 1024);
        assert!(e.history(1, 10).is_err());
        assert_eq!(e.history(0, 256).unwrap().points.len(), 256);
        assert!(e.history(0, 257).is_err());
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test]
    fn accepted_replay_reserves_capacity_against_later_live_ticks() {
        let mut e = Engine::default();
        e.apply(Event::ScheduleReplay(ReplaySpec {
            job_id: "reserved".into(),
            ticks: (1..=2)
                .map(|n| Tick {
                    event_id: format!("replay-{n}"),
                    symbol: "LAB".into(),
                    price_cents: 100,
                    sequence: n,
                })
                .collect(),
        }))
        .unwrap();
        for n in 1..=9997 {
            e.apply(Event::Tick(Tick {
                event_id: format!("live-{n}"),
                symbol: "RUST".into(),
                price_cents: 100,
                sequence: n,
            }))
            .unwrap();
        }
        assert!(
            e.apply(Event::Tick(Tick {
                event_id: "too-many".into(),
                symbol: "RUST".into(),
                price_cents: 100,
                sequence: 9998
            }))
            .is_err()
        );
        for index in 0..2 {
            e.apply(Event::ReplayStep {
                job_id: "reserved".into(),
                index,
            })
            .unwrap();
        }
        assert_eq!(e.jobs()[0].status, ReplayStatus::Completed);
        assert_eq!(e.snapshot().events, 10_000);
    }
}
