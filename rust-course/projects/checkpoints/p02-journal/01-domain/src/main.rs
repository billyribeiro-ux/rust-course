use checkpoint_p02_journal::{Error, Journal};
fn main() -> Result<(), Error> {
    let mut journal = Journal::default();
    let id = journal.add("learn ownership")?;
    journal.complete(id)?;
    for task in journal.tasks() {
        println!("{} {:?}: {}", task.id(), task.status(), task.title());
    }
    Ok(())
}
