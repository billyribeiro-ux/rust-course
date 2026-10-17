use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::{Rc, Weak};

pub const MAX_MESSAGES: usize = 4;
pub const MAX_MESSAGE_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditError {
    InvalidMessage,
    Full,
    Borrowed,
}
impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidMessage => "message must contain 1 to 64 bytes and no control characters",
            Self::Full => "audit buffer is full",
            Self::Borrowed => "audit buffer is already borrowed by an observer",
        })
    }
}
impl std::error::Error for AuditError {}

#[derive(Debug)]
struct Inner {
    enabled: Cell<bool>,
    messages: RefCell<Vec<String>>,
}

/// Two synchronous callbacks can retain handles to one bounded audit buffer.
/// This deliberately single-threaded handle is not a cross-thread synchronization primitive.
///
/// ```compile_fail
/// let audit = core_rust_labs::interior::SharedAudit::default();
/// std::thread::spawn(move || audit.record("moved to another thread"));
/// ```
#[derive(Debug, Clone)]
pub struct SharedAudit {
    inner: Rc<Inner>,
}

/// A non-owning observer: retaining this handle does not retain the messages.
#[derive(Debug, Clone)]
pub struct WeakAudit {
    inner: Weak<Inner>,
}
impl WeakAudit {
    /// A successful upgrade owns the buffer until the returned handle is dropped.
    pub fn upgrade(&self) -> Option<SharedAudit> {
        self.inner.upgrade().map(|inner| SharedAudit { inner })
    }
}
impl Default for SharedAudit {
    fn default() -> Self {
        Self {
            inner: Rc::new(Inner {
                enabled: Cell::new(true),
                messages: RefCell::new(Vec::new()),
            }),
        }
    }
}
impl SharedAudit {
    pub fn downgrade(&self) -> WeakAudit {
        WeakAudit {
            inner: Rc::downgrade(&self.inner),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.enabled.set(enabled);
    }

    /// Validate all calls, including disabled ones. Ok(false) means valid but disabled.
    /// Rejections do not add a message. No user callback runs while mutably borrowed.
    pub fn record(&self, message: &str) -> Result<bool, AuditError> {
        if message.is_empty()
            || message.len() > MAX_MESSAGE_BYTES
            || message.chars().any(char::is_control)
        {
            return Err(AuditError::InvalidMessage);
        }
        if !self.inner.enabled.get() {
            return Ok(false);
        }
        let mut messages = self
            .inner
            .messages
            .try_borrow_mut()
            .map_err(|_| AuditError::Borrowed)?;
        if messages.len() == MAX_MESSAGES {
            return Err(AuditError::Full);
        }
        messages.push(message.to_owned());
        Ok(true)
    }

    /// The observation callback runs under a shared borrow. Reentrant writes return Borrowed.
    /// Keep callbacks brief; this is synchronous observation, not an async lock.
    pub fn with_messages<R>(&self, inspect: impl FnOnce(&[String]) -> R) -> Result<R, AuditError> {
        let messages = self
            .inner
            .messages
            .try_borrow()
            .map_err(|_| AuditError::Borrowed)?;
        Ok(inspect(&messages))
    }

    /// An owned snapshot is bounded by MAX_MESSAGES and MAX_MESSAGE_BYTES.
    pub fn snapshot(&self) -> Result<Vec<String>, AuditError> {
        self.with_messages(|messages| messages.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles_share_state_and_disabled_is_distinct_from_invalid() {
        let audit = SharedAudit::default();
        let other = audit.clone();
        assert_eq!(audit.record("opened"), Ok(true));
        other.set_enabled(false);
        assert_eq!(audit.record("paused"), Ok(false));
        assert_eq!(audit.record(""), Err(AuditError::InvalidMessage));
        other.set_enabled(true);
        drop(audit);
        assert_eq!(other.record("resumed"), Ok(true));
        assert_eq!(other.snapshot().unwrap(), ["opened", "resumed"]);
    }

    #[test]
    fn full_or_invalid_input_preserves_messages() {
        let audit = SharedAudit::default();
        for _ in 0..MAX_MESSAGES {
            audit.record("entry").unwrap();
        }
        let before = audit.snapshot().unwrap();
        assert_eq!(audit.record("extra"), Err(AuditError::Full));
        for bad in [
            String::new(),
            "x".repeat(MAX_MESSAGE_BYTES + 1),
            "bad\nline".into(),
        ] {
            assert_eq!(audit.record(&bad), Err(AuditError::InvalidMessage));
        }
        assert_eq!(audit.snapshot().unwrap(), before);
    }

    #[test]
    fn reentrant_write_is_an_error_then_guard_release_allows_progress() {
        let audit = SharedAudit::default();
        audit.record("before").unwrap();
        let observed = audit.with_messages(|messages| {
            assert_eq!(audit.record("during"), Err(AuditError::Borrowed));
            messages.len()
        });
        assert_eq!(observed, Ok(1));
        assert_eq!(audit.record("after"), Ok(true));
        assert_eq!(audit.snapshot().unwrap(), ["before", "after"]);
    }

    #[test]
    fn unwinding_releases_the_observer_guard() {
        let audit = SharedAudit::default();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = audit.with_messages(|_| panic!("deliberate observer failure"));
        }));
        assert!(failure.is_err());
        assert_eq!(audit.record("after unwind"), Ok(true));
    }

    #[test]
    fn weak_observer_does_not_keep_the_buffer_alive() {
        let first = SharedAudit::default();
        first.record("retained by strong handles").unwrap();
        let second = first.clone();
        let observer = first.downgrade();
        drop(first);
        assert_eq!(observer.upgrade().unwrap().snapshot().unwrap().len(), 1);
        drop(second);
        assert!(observer.upgrade().is_none());
    }
}
