use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Done,
}
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Journal {
    tasks: Vec<Task>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Title,
    Full,
    IdOverflow,
    NotFound(u64),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Title => "invalid task title",
                Self::Full => "32-task limit reached",
                Self::IdOverflow => "task identifier exhausted",
                Self::NotFound(_) => "task identifier not found",
            }
        )
    }
}
impl std::error::Error for Error {}
impl Journal {
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }
    pub fn add(&mut self, title: &str) -> Result<u64, Error> {
        if title.trim().is_empty() || title.len() > 200 || title.chars().any(char::is_control) {
            return Err(Error::Title);
        }
        if self.tasks.len() == 32 {
            return Err(Error::Full);
        }
        let id = self
            .tasks
            .last()
            .map_or(0, |task| task.id)
            .checked_add(1)
            .ok_or(Error::IdOverflow)?;
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
            .find(|task| task.id == id)
            .ok_or(Error::NotFound(id))?;
        task.status = Status::Done;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn owns_title_and_completion_is_idempotent() {
        let mut journal = Journal::default();
        let id = {
            let text = String::from("learn");
            journal.add(&text).unwrap()
        };
        journal.complete(id).unwrap();
        journal.complete(id).unwrap();
        assert_eq!(journal.tasks()[0].title(), "learn");
        assert_eq!(journal.tasks()[0].status(), Status::Done);
    }
    #[test]
    fn rejected_input_preserves_state() {
        let mut journal = Journal::default();
        journal.add("valid").unwrap();
        let before = journal.clone();
        assert_eq!(journal.add(" "), Err(Error::Title));
        assert_eq!(journal.complete(99), Err(Error::NotFound(99)));
        assert_eq!(journal, before);
        for invalid in ["a\n".to_owned(), "é".repeat(101)] {
            assert_eq!(journal.add(&invalid), Err(Error::Title));
        }
    }
    #[test]
    fn capacity_boundary_preserves_last_state() {
        let mut journal = Journal::default();
        for _ in 0..32 {
            journal.add("task").unwrap();
        }
        let before = journal.clone();
        assert_eq!(journal.add("extra"), Err(Error::Full));
        assert_eq!(journal, before);
    }
}
