//! Transport models and pure presentation decisions, shared with the WASM client.
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Quote {
    pub symbol: String,
    pub price_cents: i64,
    pub sequence: u64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub quantity: i64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ExecutedOrder {
    pub order_id: String,
    pub symbol: String,
    pub side: String,
    pub quantity: i64,
    pub price_cents: i64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct HistoryPoint {
    pub event_position: u64,
    pub symbol: String,
    pub price_cents: i64,
    pub sequence: u64,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct HistoryPage {
    pub points: Vec<HistoryPoint>,
    pub oldest_position: Option<u64>,
    pub latest_position: Option<u64>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Job {
    pub job_id: String,
    pub next: usize,
    pub total: usize,
    pub status: String,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Snapshot {
    pub simulated: bool,
    pub cash_cents: i64,
    pub quotes: Vec<Quote>,
    pub positions: Vec<Position>,
    pub orders: Vec<ExecutedOrder>,
    pub watchlist: Vec<String>,
    pub events: u64,
}
pub fn money(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let magnitude = cents.unsigned_abs();
    format!("{sign}${}.{:02}", magnitude / 100, magnitude % 100)
}

/// An uncertain response cannot authorize inventing a new command identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Accepted,
    NotAccepted,
    Unknown,
}
pub fn write_outcome(status: Option<u16>, earlier_uncertain: bool) -> WriteOutcome {
    match status {
        Some(200..=299) => WriteOutcome::Accepted,
        Some(408 | 409 | 500..=599) | None => WriteOutcome::Unknown,
        Some(_) if earlier_uncertain => WriteOutcome::Unknown,
        Some(_) => WriteOutcome::NotAccepted,
    }
}
/// The same logical instruction retains its identity and payload on retry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingOrder {
    pub id: String,
    pub symbol: String,
    pub quantity: u64,
}
impl PendingOrder {
    pub fn body(&self) -> serde_json::Value {
        serde_json::json!({"order_id":self.id,"symbol":self.symbol,"side":"buy","quantity":self.quantity})
    }
}
/// Five LAB ticks require five consecutive representable sequence numbers.
pub fn replay_ticks(job: &str, previous: u64) -> Result<Vec<serde_json::Value>, &'static str> {
    previous
        .checked_add(5)
        .ok_or("LAB sequence is exhausted; no job was submitted.")?;
    Ok((1..=5u64).map(|i| serde_json::json!({"event_id":format!("{job}-{i}"),"symbol":"LAB","price_cents":9975+i*25,"sequence":previous+i})).collect())
}

/// The open page retains the entire replay request until its outcome is known.
/// A refreshed quote must not change the payload of an uncertain retry.
#[derive(Clone, Debug, Default)]
pub struct ReplaySubmission {
    draft: Option<(String, serde_json::Value)>,
    earlier_uncertain: bool,
}
impl ReplaySubmission {
    pub fn job_id(&self) -> Option<&str> {
        self.draft.as_ref().map(|(id, _)| id.as_str())
    }
    pub fn prepare(
        &mut self,
        new_id: impl FnOnce() -> String,
        previous: u64,
    ) -> Result<serde_json::Value, &'static str> {
        if let Some((_, body)) = &self.draft {
            return Ok(body.clone());
        }
        let job_id = new_id();
        let ticks = replay_ticks(&job_id, previous)?;
        let body = serde_json::json!({"job_id":job_id,"ticks":ticks});
        self.draft = Some((job_id, body.clone()));
        Ok(body)
    }
    pub fn resolve(&mut self, status: Option<u16>) -> WriteOutcome {
        if self.draft.is_none() {
            return WriteOutcome::Unknown;
        }
        let outcome = write_outcome(status, self.earlier_uncertain);
        if outcome == WriteOutcome::Unknown {
            self.earlier_uncertain = true;
        } else {
            self.draft = None;
            self.earlier_uncertain = false;
        }
        outcome
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_history_decodes_without_a_fabricated_cursor() {
        let page: HistoryPage =
            serde_json::from_str(r#"{"points":[],"oldest_position":null,"latest_position":null}"#)
                .unwrap();
        assert!(page.points.is_empty());
        assert_eq!(page.oldest_position, None);
        assert_eq!(page.latest_position, None);
    }
    #[test]
    fn boundary_sequences_do_not_wrap() {
        assert!(replay_ticks("j", u64::MAX).is_err());
        assert!(replay_ticks("j", u64::MAX - 4).is_err());
        let ticks = replay_ticks("j", u64::MAX - 5).unwrap();
        assert_eq!(ticks[4]["sequence"], u64::MAX);
        assert_eq!(replay_ticks("j", 0).unwrap()[0]["sequence"], 1);
    }
    #[test]
    fn uncertain_http_outcomes_remain_uncertain() {
        for status in [None, Some(408), Some(409), Some(500), Some(503)] {
            assert_eq!(write_outcome(status, false), WriteOutcome::Unknown);
        }
        assert_eq!(write_outcome(Some(200), false), WriteOutcome::Accepted);
        assert_eq!(write_outcome(Some(422), false), WriteOutcome::NotAccepted);
        assert_eq!(write_outcome(Some(401), false), WriteOutcome::NotAccepted);
    }
    #[test]
    fn retry_rejection_does_not_resolve_an_earlier_attempt() {
        for status in [401, 422, 429] {
            assert_eq!(write_outcome(Some(status), true), WriteOutcome::Unknown);
        }
        assert_eq!(write_outcome(Some(200), true), WriteOutcome::Accepted);
    }
    #[test]
    fn money_is_exact_integer_formatting_including_minimum() {
        assert_eq!(money(10005), "$100.05");
        assert_eq!(money(-1), "-$0.01");
        assert_eq!(money(i64::MIN), "-$92233720368547758.08");
    }
    #[test]
    fn replay_retry_retains_identity_and_ticks_across_new_quotes_and_rejections() {
        let mut submission = ReplaySubmission::default();
        let original = submission.prepare(|| "first".into(), 20).unwrap();
        for status in [None, Some(409), Some(422), Some(429), Some(503)] {
            assert_eq!(submission.resolve(status), WriteOutcome::Unknown);
            let retry = submission
                .prepare(
                    || panic!("a retry must not generate another identity"),
                    u64::MAX,
                )
                .unwrap();
            assert_eq!(retry, original);
            assert_eq!(submission.job_id(), Some("first"));
        }
        assert_eq!(submission.resolve(Some(200)), WriteOutcome::Accepted);
        assert_eq!(submission.job_id(), None);
        let next = submission.prepare(|| "second".into(), 25).unwrap();
        assert_ne!(next, original);
        assert_eq!(next["ticks"][0]["sequence"], 26);
    }
    #[test]
    fn replay_first_rejection_or_local_validation_does_not_hold_a_draft() {
        let mut submission = ReplaySubmission::default();
        assert!(submission.prepare(|| "exhausted".into(), u64::MAX).is_err());
        assert_eq!(submission.job_id(), None);
        submission.prepare(|| "rejected".into(), 0).unwrap();
        assert_eq!(submission.resolve(Some(422)), WriteOutcome::NotAccepted);
        assert_eq!(submission.job_id(), None);
        submission.prepare(|| "fresh".into(), 5).unwrap();
        assert_eq!(submission.resolve(Some(401)), WriteOutcome::NotAccepted);
        assert_eq!(submission.job_id(), None);
    }
}
