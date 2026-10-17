mod support;
// Tests share disposable teaching tables; serialize within this test executable.
static DATABASE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
#[ignore = "requires isolated COURSE_SCHEDULER_TEST_DATABASE_URL"]
async fn crash_redelivery_fencing_dedup_cancellation() {
    use p14_scheduler::*;
    let _fixture_guard = DATABASE_TEST_LOCK.lock().await;
    let pool = support::isolated_pool(
        "COURSE_SCHEDULER_TEST_DATABASE_URL",
        "rust_course_test_scheduler",
    )
    .await;
    sqlx::migrate!().run(&pool).await.unwrap();
    sqlx::raw_sql("TRUNCATE job_effects,jobs")
        .execute(&pool)
        .await
        .unwrap();
    assert!(enqueue(&pool, "a", "report").await.unwrap());
    assert!(!enqueue(&pool, "a", "report").await.unwrap());
    assert!(enqueue(&pool, "a", "other").await.is_err());
    let stale = claim(&pool, 0).await.unwrap().unwrap();
    assert!(claim(&pool, 1).await.unwrap().is_none());
    // Worker disappears; the simulated clock advances past its lease.
    let fresh = claim(&pool, LEASE_MS).await.unwrap().unwrap();
    assert!(fresh.token > stale.token);
    assert!(!complete(&pool, &stale, LEASE_MS + 1).await.unwrap());
    assert!(complete(&pool, &fresh, LEASE_MS + 1).await.unwrap());
    assert!(!complete(&pool, &fresh, LEASE_MS + 2).await.unwrap());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM job_effects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    enqueue(&pool, "b", "cancel").await.unwrap();
    let b = claim(&pool, 10_000).await.unwrap().unwrap();
    assert!(cancel(&pool, "b").await.unwrap());
    assert!(!complete(&pool, &b, 10_001).await.unwrap());
    enqueue(&pool, "c", "retry").await.unwrap();
    let c = claim(&pool, 20_000).await.unwrap().unwrap();
    assert!(fail(&pool, &c, 20_001).await.unwrap());
    assert!(claim(&pool, 20_002).await.unwrap().is_none());
    assert_eq!(claim(&pool, 22_001).await.unwrap().unwrap().attempts, 2);
    enqueue(&pool, "d", "concurrent claim").await.unwrap();
    let (a, b) = tokio::join!(claim(&pool, 0), claim(&pool, 0));
    assert_eq!(
        usize::from(a.unwrap().is_some()) + usize::from(b.unwrap().is_some()),
        1
    );
    // Fill only this explicitly disposable database to the retained-job budget.
    sqlx::query(
        "INSERT INTO jobs(id,payload) SELECT 'budget-'||n,'fixture' FROM generate_series(1,10000-(SELECT count(*) FROM jobs)) n",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        matches!(enqueue(&pool,"over-budget","fixture").await,Err(sqlx::Error::Protocol(message)) if message=="queue budget reached")
    );
    assert!(!enqueue(&pool, "a", "report").await.unwrap());
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires isolated COURSE_SCHEDULER_TEST_DATABASE_URL"]
async fn lease_fields_cannot_rewrite_durable_payload_or_retry_count() {
    use p14_scheduler::*;
    let _fixture_guard = DATABASE_TEST_LOCK.lock().await;
    let pool = support::isolated_pool(
        "COURSE_SCHEDULER_TEST_DATABASE_URL",
        "rust_course_test_scheduler",
    )
    .await;
    sqlx::migrate!().run(&pool).await.unwrap();
    // The shared fixture mutex excludes the other database test during table reset.
    sqlx::raw_sql("TRUNCATE job_effects,jobs")
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, "immutable", "durable payload")
        .await
        .unwrap();
    let mut lease = claim(&pool, 0).await.unwrap().unwrap();
    lease.payload = "caller substitution".into();
    assert!(complete(&pool, &lease, 1).await.unwrap());
    let effect: String =
        sqlx::query_scalar("SELECT payload FROM job_effects WHERE job_id='immutable'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(effect, "durable payload");
    enqueue(&pool, "retry-authority", "retry input")
        .await
        .unwrap();
    let mut lease = claim(&pool, 10_000).await.unwrap().unwrap();
    assert_eq!(lease.attempts, 1);
    lease.attempts = 3;
    assert!(fail(&pool, &lease, 10_001).await.unwrap());
    assert!(claim(&pool, 12_000).await.unwrap().is_none());
    let second = claim(&pool, 12_001).await.unwrap().unwrap();
    assert_eq!(second.attempts, 2);
    // Expiry without a reclaim also removes authority; cancellation revokes a current token.
    assert!(!complete(&pool, &second, 17_001).await.unwrap());
    assert!(!fail(&pool, &second, 17_001).await.unwrap());
    let third = claim(&pool, 17_001).await.unwrap().unwrap();
    assert_eq!(third.attempts, 3);
    assert!(claim(&pool, 22_001).await.unwrap().is_none());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id='retry-authority'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "dead");
    // Inject an effect constraint failure after authority validation. The transaction
    // must leave both effect and completion uncommitted. Remove only this test constraint.
    enqueue(&pool, "atomic-effect", "fail-transaction")
        .await
        .unwrap();
    let lease = claim(&pool, 30_000).await.unwrap().unwrap();
    sqlx::raw_sql("ALTER TABLE job_effects ADD CONSTRAINT course_injected_failure CHECK(payload <> 'fail-transaction')")
        .execute(&pool).await.unwrap();
    let outcome = complete(&pool, &lease, 30_001).await;
    sqlx::raw_sql("ALTER TABLE job_effects DROP CONSTRAINT course_injected_failure")
        .execute(&pool)
        .await
        .unwrap();
    assert!(outcome.is_err());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id='atomic-effect'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let effects: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job_effects WHERE job_id='atomic-effect'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "running");
    assert_eq!(effects, 0);
    assert!(complete(&pool, &lease, 30_002).await.unwrap());
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires isolated COURSE_SCHEDULER_TEST_DATABASE_URL"]
async fn completion_samples_clock_after_waiting_for_its_row_lock() {
    use p14_scheduler::*;
    use std::sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    };
    let _fixture_guard = DATABASE_TEST_LOCK.lock().await;
    let pool = support::isolated_pool(
        "COURSE_SCHEDULER_TEST_DATABASE_URL",
        "rust_course_test_scheduler",
    )
    .await;
    sqlx::migrate!().run(&pool).await.unwrap();
    sqlx::raw_sql("TRUNCATE job_effects,jobs")
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, "delayed-lock", "clock fixture")
        .await
        .unwrap();
    let lease = claim(&pool, 0).await.unwrap().unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM jobs WHERE id='delayed-lock' FOR UPDATE")
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let logical_clock = Arc::new(AtomicI64::new(1));
    let worker_clock = Arc::clone(&logical_clock);
    let worker_pool = pool.clone();
    let worker = tokio::spawn(async move {
        complete_with_clock(&worker_pool, &lease, move || {
            Ok(worker_clock.load(Ordering::SeqCst))
        })
        .await
    });
    // Observe the actual PostgreSQL lock wait, not a guessed sleep duration.
    let observed_wait = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks l JOIN pg_stat_activity a USING(pid) WHERE NOT l.granted AND l.pid<>pg_backend_pid() AND a.datname=current_database())")
                .fetch_one(&pool).await.unwrap();
            if waiting { break; }
            tokio::task::yield_now().await;
        }
    }).await;
    // Advance past expiry while the worker demonstrably cannot yet lock the row.
    logical_clock.store(LEASE_MS, Ordering::SeqCst);
    blocker.rollback().await.unwrap();
    let applied = worker.await.unwrap().unwrap();
    assert!(
        observed_wait.is_ok(),
        "worker did not reach the controlled row-lock wait"
    );
    assert!(
        !applied,
        "pre-wait time must not authorize an expired lease"
    );
    let effects: i64 = sqlx::query_scalar("SELECT count(*) FROM job_effects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(effects, 0);
    pool.close().await;
}
