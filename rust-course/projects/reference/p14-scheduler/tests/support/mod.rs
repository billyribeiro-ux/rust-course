//! Guard only disposable local fixtures; a name is not proof of ownership.
use sqlx::{
    PgPool,
    postgres::{PgConnectOptions, PgPoolOptions},
};
use std::{net::IpAddr, str::FromStr, time::Duration};

fn checked_options(url: &str, database: &str) -> Result<PgConnectOptions, &'static str> {
    let options = PgConnectOptions::from_str(url).map_err(|_| "invalid test database URL")?;
    if options.get_database() != Some(database) {
        return Err("test URL must select the exact documented disposable database");
    }
    let local = match options.get_socket() {
        Some(socket) => socket.is_absolute(),
        None => options
            .get_host()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback()),
    };
    if !local {
        return Err("test connection must use a loopback IP or absolute Unix socket path");
    }
    Ok(options)
}

pub async fn isolated_pool(variable: &str, database: &'static str) -> PgPool {
    // Test assertions deliberately fail the fixture setup before any migration or reset.
    // Do not print the URL: an accidentally supplied URL could contain a password.
    let url = std::env::var(variable)
        .map_err(|_| "test URL is missing or is not UTF-8")
        .expect("set the documented disposable test URL");
    let options = checked_options(&url, database).expect("refusing unsafe test connection");
    PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(5))
        .after_connect(move |connection, _| {
            Box::pin(async move {
                let actual: String = sqlx::query_scalar("SELECT current_database()")
                    .fetch_one(connection)
                    .await?;
                if actual != database {
                    return Err(sqlx::Error::Protocol(
                        "refusing unexpected connected database".into(),
                    ));
                }
                Ok(())
            })
        })
        .connect_with(options)
        .await
        .expect("could not connect to the disposable fixture")
}

#[test]
fn effective_database_name_must_match_not_merely_appear_in_url() {
    let database = "rust_course_test_issues";
    for url in [
        "postgresql://rust_course_test:fixture@127.0.0.1/application",
        "postgresql://fixture:rust_course_test@127.0.0.1/application",
        "postgresql://127.0.0.1/application?application_name=rust_course_test",
        "postgresql://127.0.0.1/rust_course_test_issues_backup",
        "postgresql://127.0.0.1/rust_course_test_issues?dbname=application",
        "postgresql://127.0.0.1/rust_course_test_scheduler",
    ] {
        assert!(
            url.contains("rust_course_test"),
            "the former guard admitted this case"
        );
        assert!(checked_options(url, database).is_err());
    }
}

#[test]
fn parsed_remote_hosts_and_query_overrides_are_rejected_without_connecting() {
    let database = "rust_course_test_issues";
    for url in [
        "postgresql://example.invalid/rust_course_test_issues",
        "postgresql://127.0.0.1/rust_course_test_issues?host=example.invalid",
        "postgresql://127.0.0.1/rust_course_test_issues?hostaddr=192.0.2.1",
    ] {
        assert!(checked_options(url, database).is_err());
    }
}

#[test]
fn explicit_local_transports_and_exact_decoded_names_are_accepted() {
    let database = "rust_course_test_issues";
    for url in [
        "postgresql://127.0.0.1/rust_course_test_issues",
        "postgresql://[::1]/rust_course_test_issues",
        "postgresql:///rust_course_test_issues?host=/tmp",
        "postgresql://127.0.0.1/rust%5Fcourse%5Ftest%5Fissues",
    ] {
        assert!(
            checked_options(url, database).is_ok(),
            "rejected a local fixture transport"
        );
    }
}
