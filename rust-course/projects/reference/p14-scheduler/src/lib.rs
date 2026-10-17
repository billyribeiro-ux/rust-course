//! At-least-once leases; one idempotent database effect, not arbitrary exactly-once work.
use serde::Serialize;
use sqlx::{PgPool, Row};
pub const LEASE_MS: i64 = 5_000;
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Lease {
    pub id: String,
    pub payload: String,
    pub token: i64,
    pub attempts: i32,
}
pub async fn enqueue(pool: &PgPool, id: &str, payload: &str) -> Result<bool, sqlx::Error> {
    if id.is_empty() || id.len() > 80 || payload.is_empty() || payload.len() > 1000 {
        return Err(sqlx::Error::Protocol("invalid job input".into()));
    }
    let mut tx = pool.begin().await?;
    // Serialize admission only, so concurrent insertions cannot exceed the durable budget.
    sqlx::query("SELECT pg_advisory_xact_lock(14001)")
        .execute(&mut *tx)
        .await?;
    if let Some(existing) = sqlx::query_scalar::<_, String>("SELECT payload FROM jobs WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
    {
        if existing != payload {
            return Err(sqlx::Error::Protocol(
                "id reused with different payload".into(),
            ));
        }
        return Ok(false);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&mut *tx)
        .await?;
    if count >= 10_000 {
        return Err(sqlx::Error::Protocol("queue budget reached".into()));
    }
    sqlx::query("INSERT INTO jobs(id,payload) VALUES($1,$2)")
        .bind(id)
        .bind(payload)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn claim(pool: &PgPool, now: i64) -> Result<Option<Lease>, sqlx::Error> {
    // One atomic statement claims the oldest eligible job, skipping locked candidates.
    sqlx::query(
        "UPDATE jobs SET status='dead' WHERE status='running' AND lease_until<=$1 AND attempts>=3",
    )
    .bind(now)
    .execute(pool)
    .await?;
    sqlx::query_as::<_,Lease>("UPDATE jobs SET status='running',attempts=attempts+1,token=token+1,lease_until=$2 WHERE id=(SELECT id FROM jobs WHERE status IN ('queued','running') AND due<=$1 AND lease_until<=$1 AND attempts<3 ORDER BY due,id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,payload,token,attempts").bind(now).bind(now.saturating_add(LEASE_MS)).fetch_optional(pool).await
}
/// Complete at an explicitly supplied logical time (deterministic models/tests).
pub async fn complete(pool: &PgPool, lease: &Lease, now: i64) -> Result<bool, sqlx::Error> {
    complete_with_clock(pool, lease, || Ok(now)).await
}
/// Sample the clock after acquiring the job row lock, before accepting authority.
/// The clock must use the same time domain as claim. This is not a clock-skew remedy.
pub async fn complete_with_clock(
    pool: &PgPool,
    lease: &Lease,
    clock: impl FnOnce() -> Result<i64, &'static str>,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row =
        sqlx::query("SELECT status,token,lease_until,payload FROM jobs WHERE id=$1 FOR UPDATE")
            .bind(&lease.id)
            .fetch_one(&mut *tx)
            .await?;
    let now = clock().map_err(|message| sqlx::Error::Protocol(message.into()))?;
    if row.get::<String, _>("status") != "running"
        || row.get::<i64, _>("token") != lease.token
        || row.get::<i64, _>("lease_until") <= now
    {
        return Ok(false);
    }
    sqlx::query(
        "INSERT INTO job_effects(job_id,payload) VALUES($1,$2) ON CONFLICT(job_id) DO NOTHING",
    )
    .bind(&lease.id)
    // The returned lease is caller-owned; durable input remains authoritative.
    .bind(row.get::<String, _>("payload"))
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE jobs SET status='done' WHERE id=$1")
        .bind(&lease.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}
/// Retry at a supplied logical time; the live worker does not use this model API.
/// A real external-effect worker needs an explicit clock/expiry policy at its effect boundary.
pub async fn fail(pool: &PgPool, lease: &Lease, now: i64) -> Result<bool, sqlx::Error> {
    // Derive backoff from the persisted attempt count, not mutable lease metadata.
    // Numeric arithmetic plus LEAST preserves saturation at the i64 timestamp limit.
    let result = sqlx::query("UPDATE jobs SET status=CASE WHEN attempts>=3 THEN 'dead' ELSE 'queued' END,due=LEAST(9223372036854775807::numeric,$3::bigint::numeric+(1000::bigint << LEAST(GREATEST(attempts,0),3)))::bigint,lease_until=0 WHERE id=$1 AND token=$2 AND status='running' AND lease_until>$3")
        .bind(&lease.id)
        .bind(lease.token)
        .bind(now)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}
/// Cancellation revokes the lease. It cannot undo an external action already performed.
pub async fn cancel(pool: &PgPool, id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE jobs SET status='cancelled',token=token+1 WHERE id=$1 AND status IN ('queued','running')").bind(id).execute(pool).await?.rows_affected()==1)
}
pub fn now_ms() -> Result<i64, &'static str> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock predates Unix epoch")?;
    i64::try_from(elapsed.as_millis()).map_err(|_| "system clock exceeds timestamp representation")
}

pub mod http;
