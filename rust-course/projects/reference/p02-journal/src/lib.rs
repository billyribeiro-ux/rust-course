//! A small versioned task journal. Titles are UTF-8, single-line, 1–200 bytes.
//! The storage format is `TASKS\t1` followed by tab-separated id/status/title rows.
//! ```
//! use task_journal::Journal;
//! let mut journal = Journal::default();
//! let id = journal.add("Learn borrowing")?;
//! journal.complete(id)?;
//! let restored = Journal::decode(&journal.encode())?;
//! assert_eq!(restored.tasks(), journal.tasks());
//! # Ok::<(), task_journal::Error>(())
//! ```
use std::{
    collections::HashSet,
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};
const MAX_FILE_BYTES: u64 = 1_048_576;
const MAX_TASKS: usize = 10_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Open,
    Done,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    id: u64,
    title: String,
    status: Status,
}
impl Task {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn status(&self) -> Status {
        self.status
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Journal {
    tasks: Vec<Task>,
}
#[derive(Debug)]
pub enum Error {
    Invalid(String),
    NotFound(u64),
    Io(io::Error),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "invalid journal: {s}"),
            Self::NotFound(id) => write!(f, "task {id} does not exist"),
            Self::Io(e) => write!(f, "storage: {e}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Io(e) = self {
            Some(e)
        } else {
            None
        }
    }
}
impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
fn validate_title(title: &str) -> Result<(), Error> {
    if title.trim().is_empty() || title.len() > 200 || title.chars().any(char::is_control) {
        return Err(Error::Invalid(
            "title must contain 1–200 bytes and no control characters".into(),
        ));
    }
    Ok(())
}
impl Journal {
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }
    pub fn add(&mut self, title: &str) -> Result<u64, Error> {
        validate_title(title)?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(Error::Invalid("task limit reached".into()));
        }
        let id = self
            .tasks
            .iter()
            .map(|t| t.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("identifier space exhausted".into()))?;
        self.tasks.push(Task {
            id,
            title: title.to_owned(),
            status: Status::Open,
        });
        Ok(id)
    }
    pub fn complete(&mut self, id: u64) -> Result<(), Error> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(Error::NotFound(id))?;
        task.status = Status::Done;
        Ok(())
    }
    pub fn remove(&mut self, id: u64) -> Result<(), Error> {
        let position = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(Error::NotFound(id))?;
        self.tasks.remove(position);
        Ok(())
    }
    pub fn encode(&self) -> String {
        let mut output = String::from("TASKS\t1\n");
        for task in &self.tasks {
            let status = match task.status {
                Status::Open => "open",
                Status::Done => "done",
            };
            use std::fmt::Write;
            writeln!(output, "{}\t{status}\t{}", task.id, task.title)
                .expect("writing String cannot fail");
        }
        output
    }
    pub fn decode(input: &str) -> Result<Self, Error> {
        if input.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid("file exceeds 1 MiB".into()));
        }
        let mut lines = input.lines();
        if lines.next() != Some("TASKS\t1") {
            return Err(Error::Invalid(
                "unsupported or missing format header".into(),
            ));
        }
        let mut journal = Self::default();
        let mut seen = HashSet::new();
        for (offset, row) in lines.enumerate() {
            let fail = || Error::Invalid(format!("malformed row {}", offset + 2));
            let columns: Vec<&str> = row.split('\t').collect();
            let [id, status, title] = columns.as_slice() else {
                return Err(fail());
            };
            let id: u64 = id.parse().map_err(|_| fail())?;
            if id == 0 || !seen.insert(id) || journal.tasks.len() >= MAX_TASKS {
                return Err(fail());
            }
            let status = match *status {
                "open" => Status::Open,
                "done" => Status::Done,
                _ => return Err(fail()),
            };
            validate_title(title)?;
            journal.tasks.push(Task {
                id,
                title: (*title).to_owned(),
                status,
            });
        }
        Ok(journal)
    }
    /// A missing path starts a new journal. Other read errors are preserved.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid("file exceeds 1 MiB".into()));
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("file is not UTF-8".into()))?;
        Self::decode(text)
    }
    /// Writes a sibling temporary file then renames it. Designed for one writer.
    /// No locking or parent-directory fsync: not a database durability guarantee.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        let bytes = self.encode();
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(Error::Invalid("file exceeds 1 MiB".into()));
        }
        let mut temporary_name = path.as_os_str().to_os_string();
        temporary_name.push(format!(".{}.tmp", std::process::id()));
        let temporary = std::path::PathBuf::from(temporary_name);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let result = (|| -> io::Result<()> {
            file.write_all(bytes.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(Error::from)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_transitions() {
        let mut j = Journal::default();
        let a = j.add("read 🦀").unwrap();
        let b = j.add("write tests").unwrap();
        j.complete(a).unwrap();
        j.complete(a).unwrap();
        j.remove(b).unwrap();
        assert_eq!(Journal::decode(&j.encode()).unwrap(), j);
        assert_eq!(j.tasks()[0].status(), Status::Done);
        assert!(matches!(j.complete(99), Err(Error::NotFound(99))));
    }
    #[test]
    fn rejects_corruption_without_partial_success() {
        for input in [
            "TASKS\t2\n",
            "TASKS\t1\n1\topen\tx\n1\tdone\ty\n",
            "TASKS\t1\n0\topen\tx\n",
            "TASKS\t1\n1\tmaybe\tx\n",
            "TASKS\t1\n1\topen\t\n",
        ] {
            assert!(Journal::decode(input).is_err(), "{input}");
        }
        let mut j = Journal::default();
        assert!(j.add(" ").is_err());
        assert!(j.add("one\ntwo").is_err());
        assert!(j.tasks().is_empty());
    }
    #[test]
    fn id_overflow_is_an_error() {
        let mut j = Journal::decode("TASKS\t1\n18446744073709551615\topen\tx\n").unwrap();
        assert!(j.add("next").is_err());
    }
}
