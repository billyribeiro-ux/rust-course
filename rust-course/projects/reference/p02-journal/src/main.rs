use std::{
    io::{self, Write},
    path::Path,
    process::ExitCode,
};
use task_journal::{Journal, Status};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [path, command, rest @ ..] = args.as_slice() else {
        return Err("usage: task-journal <file> list|add <title>|done <id>|remove <id>".into());
    };
    let path = Path::new(path);
    let mut journal = Journal::load(path)?;
    let command = command.to_str().ok_or("command must be valid UTF-8")?;
    let mut stdout = io::stdout().lock();
    match (command, rest) {
        ("list", []) => {
            for task in journal.tasks() {
                writeln!(
                    stdout,
                    "{} [{}] {}",
                    task.id(),
                    if task.status() == Status::Done {
                        "x"
                    } else {
                        " "
                    },
                    task.title()
                )?;
            }
        }
        ("add", [title]) => {
            let id = journal.add(title.to_str().ok_or("title must be valid UTF-8")?)?;
            journal.save(path)?;
            // Saving already succeeded; an output failure does not undo that save.
            writeln!(stdout, "added {id}")?;
        }
        ("done", [id]) => {
            journal.complete(id.to_str().ok_or("id must be valid UTF-8")?.parse()?)?;
            journal.save(path)?;
        }
        ("remove", [id]) => {
            journal.remove(id.to_str().ok_or("id must be valid UTF-8")?.parse()?)?;
            journal.save(path)?;
        }
        _ => return Err("invalid command or argument count".into()),
    }
    stdout.flush()?;
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = writeln!(io::stderr().lock(), "journal: {e}");
            ExitCode::FAILURE
        }
    }
}
