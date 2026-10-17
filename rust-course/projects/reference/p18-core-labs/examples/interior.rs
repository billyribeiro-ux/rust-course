use core_rust_labs::interior::{AuditError, SharedAudit};

fn main() -> Result<(), AuditError> {
    let first_callback = SharedAudit::default();
    let second_callback = first_callback.clone();
    let observer = first_callback.downgrade();
    first_callback.record("first callback")?;
    second_callback.record("second callback")?;
    first_callback.with_messages(|messages| {
        println!("observed messages: {}", messages.len());
        println!(
            "reentrant write: {:?}",
            second_callback.record("while observing")
        );
    })?;
    first_callback.set_enabled(false);
    println!(
        "recorded while disabled: {}",
        second_callback.record("ignored")?
    );
    println!("snapshot: {:?}", first_callback.snapshot()?);
    drop(first_callback);
    drop(second_callback);
    println!(
        "observer can still upgrade: {}",
        observer.upgrade().is_some()
    );
    Ok(())
}
