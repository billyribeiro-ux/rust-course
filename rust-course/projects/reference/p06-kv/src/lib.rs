//! Single-writer educational log store. CRC checks complete declared records, not tampering.
//! An incomplete declared suffix is discarded; bounded length damage can look incomplete.
//! Successful writes call sync_all; hardware/filesystem power-loss guarantees vary.
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 4] = b"KV01";
const HEADER: usize = 16;
pub const MAX_KEY: usize = 4096;
pub const MAX_VALUE: usize = 1_048_576;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// IEEE CRC-32, reflected polynomial; deliberately transparent rather than optimized.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn encode(key: &[u8], value: Option<&[u8]>) -> io::Result<Vec<u8>> {
    if key.is_empty() || key.len() > MAX_KEY || value.is_some_and(|v| v.len() > MAX_VALUE) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "key/value length outside bounds",
        ));
    }
    let mut record = Vec::with_capacity(HEADER + key.len() + value.map_or(0, <[u8]>::len));
    record.extend_from_slice(MAGIC);
    record.extend_from_slice(&(key.len() as u32).to_le_bytes());
    record.extend_from_slice(&value.map_or(u32::MAX, |v| v.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(key);
    if let Some(value) = value {
        record.extend_from_slice(value);
    }
    let checksum = crc32(&[&record[..12], &record[16..]].concat());
    record[12..16].copy_from_slice(&checksum.to_le_bytes());
    Ok(record)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Recovery {
    pub entries: BTreeMap<Vec<u8>, Vec<u8>>,
    pub valid_bytes: u64,
    pub records: usize,
    pub torn_tail: bool,
}

/// Replay bounded individual records. The live key map itself grows with the data set.
/// An incomplete declared header/payload returns the validated prefix. Detected magic,
/// length-bound and checksum violations are errors. CRC is checked only after reading
/// the declared payload: a damaged but bounded length can resemble an interrupted append.
pub fn replay(mut input: impl Read) -> io::Result<Recovery> {
    let mut result = Recovery::default();
    loop {
        let mut header = [0u8; HEADER];
        let n = read_prefix(&mut input, &mut header)?;
        if n == 0 {
            return Ok(result);
        }
        if n != HEADER {
            result.torn_tail = true;
            return Ok(result);
        }
        if &header[..4] != MAGIC {
            return Err(invalid("bad record magic"));
        }
        let key_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let raw_value_len = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let deleted = raw_value_len == u32::MAX;
        let value_len = if deleted { 0 } else { raw_value_len as usize };
        if key_len == 0 || key_len > MAX_KEY || value_len > MAX_VALUE {
            return Err(invalid("invalid record length"));
        }
        let mut payload = vec![0; key_len + value_len];
        if read_prefix(&mut input, &mut payload)? != payload.len() {
            result.torn_tail = true;
            return Ok(result);
        }
        let expected = u32::from_le_bytes(header[12..16].try_into().unwrap());
        if crc32(&[&header[..12], &payload].concat()) != expected {
            return Err(invalid("record checksum mismatch"));
        }
        let key = payload[..key_len].to_vec();
        if deleted {
            result.entries.remove(&key);
        } else {
            result.entries.insert(key, payload[key_len..].to_vec());
        }
        result.valid_bytes += (HEADER + payload.len()) as u64;
        result.records += 1;
    }
}

fn read_prefix(input: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut used = 0;
    while used < buf.len() {
        match input.read(&mut buf[used..]) {
            Ok(0) => break,
            Ok(n) => used += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(used)
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn sync_parent(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        File::open(parent)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub struct Store {
    path: PathBuf,
    file: File,
    _lock: File, // Separate inode: compaction replaces the data file, never this lock.
    entries: BTreeMap<Vec<u8>, Vec<u8>>,
    poisoned: bool,
    pub recovered_torn_tail: bool,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        // Freeze the opening cwd before retaining paths used by later compaction.
        // This does not resolve symlinks or protect against external path replacement.
        let path = std::path::absolute(path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(sibling(&path, ".lock"))?;
        lock.try_lock().map_err(io::Error::other)?;
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)?;
        let recovery = replay(&file)?;
        if recovery.torn_tail {
            file.set_len(recovery.valid_bytes)?;
        }
        file.sync_all()?;
        sync_parent(&path)?;
        Ok(Self {
            path,
            file,
            _lock: lock,
            entries: recovery.entries,
            poisoned: false,
            recovered_torn_tail: recovery.torn_tail,
        })
    }
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.entries.get(key).map(Vec::as_slice)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.change(key, Some(value))
    }
    pub fn delete(&mut self, key: &[u8]) -> io::Result<()> {
        self.change(key, None)
    }
    fn ready(&self) -> io::Result<()> {
        if self.poisoned {
            Err(io::Error::other(
                "store write failed; drop and reopen before further mutations",
            ))
        } else {
            Ok(())
        }
    }
    fn change(&mut self, key: &[u8], value: Option<&[u8]>) -> io::Result<()> {
        self.ready()?;
        let record = encode(key, value)?;
        if let Err(error) = self
            .file
            .write_all(&record)
            .and_then(|()| self.file.sync_all())
        {
            self.poisoned = true;
            return Err(error);
        }
        if let Some(value) = value {
            self.entries.insert(key.to_vec(), value.to_vec());
        } else {
            self.entries.remove(key);
        }
        Ok(())
    }
    /// Unix rename + parent sync protocol. Windows replacement semantics are not modeled.
    #[cfg(unix)]
    pub fn compact(&mut self) -> io::Result<()> {
        self.ready()?;
        let temporary = sibling(&self.path, ".compact");
        let mut next = OpenOptions::new()
            .read(true)
            .append(true)
            .create_new(true)
            .open(&temporary)?;
        for (key, value) in &self.entries {
            next.write_all(&encode(key, Some(value))?)?;
        }
        next.sync_all()?;
        fs::rename(&temporary, &self.path)?;
        self.file = next;
        if let Err(error) = sync_parent(&self.path) {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "rust-course-kv-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn crc_known_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }
    #[test]
    fn every_torn_suffix_recovers_only_the_complete_prefix() {
        let first = encode(b"a", Some(b"one")).unwrap();
        let second = encode(b"b", Some(b"two")).unwrap();
        for cut in 1..second.len() {
            let bytes = [&first[..], &second[..cut]].concat();
            let recovered = replay(&bytes[..]).unwrap();
            assert_eq!(recovered.records, 1);
            assert!(recovered.torn_tail);
            assert_eq!(recovered.valid_bytes, first.len() as u64);
        }
    }
    #[test]
    fn checksum_mismatch_with_complete_declared_payload_is_rejected() {
        let mut bytes = encode(b"a", Some(b"one")).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert_eq!(
            replay(&bytes[..]).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    #[test]
    fn malicious_length_is_rejected_before_allocation() {
        let mut bytes = encode(b"a", Some(b"one")).unwrap();
        bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(replay(&bytes[..]).is_err());
    }
    #[test]
    fn overwritten_deleted_and_empty_values_replay() {
        let bytes = [
            encode(b"a", Some(b"one")).unwrap(),
            encode(b"a", Some(b"")).unwrap(),
            encode(b"b", Some(b"two")).unwrap(),
            encode(b"b", None).unwrap(),
        ]
        .concat();
        let recovered = replay(&bytes[..]).unwrap();
        assert_eq!(recovered.entries.get(b"a".as_slice()), Some(&vec![]));
        assert_eq!(recovered.entries.len(), 1);
    }
    #[test]
    fn read_error_is_not_misclassified_as_torn_tail() {
        struct Failed;
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("injected read failure"))
            }
        }
        assert_eq!(replay(Failed).unwrap_err().kind(), io::ErrorKind::Other);
    }
    #[test]
    fn reopen_repairs_tail_and_can_append() {
        let dir = Scratch::new();
        let p = dir.0.join("data");
        {
            let mut s = Store::open(&p).unwrap();
            s.put(b"a", b"one").unwrap();
            assert!(Store::open(&p).is_err());
        }
        OpenOptions::new()
            .append(true)
            .open(&p)
            .unwrap()
            .write_all(b"KV")
            .unwrap();
        {
            let mut s = Store::open(&p).unwrap();
            assert!(s.recovered_torn_tail);
            s.put(b"b", b"two").unwrap();
        }
        let s = Store::open(&p).unwrap();
        assert_eq!(s.get(b"a"), Some(b"one".as_slice()));
        assert_eq!(s.get(b"b"), Some(b"two".as_slice()));
    }
    #[test]
    fn failed_write_keeps_acknowledged_state_and_poison_requires_reopen() {
        let dir = Scratch::new();
        let p = dir.0.join("data");
        let mut s = Store::open(&p).unwrap();
        s.put(b"a", b"one").unwrap();
        // Fault injection: replace only the data handle with a read-only descriptor.
        // Keep the real lock held and exercise Store::change's actual error path.
        s.file = File::open(&p).unwrap();
        assert!(s.put(b"b", b"two").is_err());
        assert!(s.poisoned);
        assert_eq!(s.get(b"b"), None);
        assert_eq!(
            s.put(b"c", b"three").unwrap_err().kind(),
            io::ErrorKind::Other
        );
        assert_eq!(s.get(b"a"), Some(b"one".as_slice()));
        drop(s);
        let mut s = Store::open(&p).unwrap();
        s.put(b"b", b"two").unwrap();
        assert_eq!(s.len(), 2);
    }
    #[cfg(unix)]
    #[test]
    fn stale_compaction_file_is_visible_and_original_remains_writable() {
        let dir = Scratch::new();
        let p = dir.0.join("data");
        let mut s = Store::open(&p).unwrap();
        s.put(b"a", b"one").unwrap();
        fs::write(sibling(&p, ".compact"), b"uncommitted temporary bytes").unwrap();
        assert_eq!(
            s.compact().unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(
            fs::read(sibling(&p, ".compact")).unwrap(),
            b"uncommitted temporary bytes"
        );
        s.put(b"b", b"two").unwrap();
        drop(s);
        assert_eq!(Store::open(&p).unwrap().len(), 2);
    }
    #[cfg(unix)]
    #[test]
    fn compaction_preserves_state_and_lock() {
        let dir = Scratch::new();
        let p = dir.0.join("data");
        let mut s = Store::open(&p).unwrap();
        s.put(b"a", b"old").unwrap();
        s.put(b"a", b"new").unwrap();
        s.put(b"b", b"gone").unwrap();
        s.delete(b"b").unwrap();
        let before = fs::metadata(&p).unwrap().len();
        s.compact().unwrap();
        assert!(fs::metadata(&p).unwrap().len() < before);
        assert!(Store::open(&p).is_err());
        s.put(b"c", b"after").unwrap();
        drop(s);
        let s = Store::open(&p).unwrap();
        assert_eq!(s.get(b"a"), Some(b"new".as_slice()));
        assert_eq!(s.len(), 2);
    }
}
