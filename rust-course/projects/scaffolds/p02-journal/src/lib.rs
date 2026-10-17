//! Learner task: model tasks privately and implement validated persistence text.
#[derive(Debug, Default)]
pub struct Journal {/* choose a representation */}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    NotFound(u64),
}
impl Journal {
    pub fn add(&mut self, title: &str) -> Result<u64, Error> {
        let _ = title;
        todo!("validate before mutation")
    }
    pub fn complete(&mut self, id: u64) -> Result<(), Error> {
        let _ = id;
        todo!("find id and change status")
    }
    pub fn encode(&self) -> String {
        todo!("TASKS\t1 header and id/status/title rows")
    }
    pub fn decode(text: &str) -> Result<Self, Error> {
        let _ = text;
        todo!("reject corrupt rows and duplicate IDs")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let mut j = Journal::default();
        let id = j.add("learn").unwrap();
        j.complete(id).unwrap();
        let text = j.encode();
        assert_eq!(Journal::decode(&text).unwrap().encode(), text);
    }
    #[test]
    fn rejects_empty() {
        assert!(Journal::default().add(" ").is_err());
    }
    #[test]
    fn rejects_version() {
        assert!(Journal::decode("TASKS\t2\n").is_err());
    }
    #[test]
    fn rejects_duplicate() {
        assert!(Journal::decode("TASKS\t1\n1\topen\tx\n1\tdone\ty\n").is_err());
    }
}
