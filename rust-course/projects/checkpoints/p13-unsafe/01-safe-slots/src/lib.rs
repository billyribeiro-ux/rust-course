//! A safe ownership oracle before implementing any unsafe storage.
#![forbid(unsafe_code)]
pub struct Slots<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Slots<T, N> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<T, const N: usize> Slots<T, N> {
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.0.len() == N {
            Err(value)
        } else {
            self.0.push(value);
            Ok(())
        }
    }
    pub fn pop(&mut self) -> Option<T> {
        self.0.pop()
    }
    pub fn as_slice(&self) -> &[T] {
        &self.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};
    #[test]
    fn capacity_failure_returns_ownership_unchanged() {
        let mut slots = Slots::<String, 1>::default();
        slots.push("first".into()).unwrap();
        assert_eq!(slots.push("second".into()), Err("second".into()));
        assert_eq!(slots.as_slice(), ["first"]);
        assert_eq!(slots.pop(), Some("first".into()));
        assert_eq!(slots.pop(), None);
    }
    #[test]
    fn zero_capacity_and_zero_sized_values_are_logical_elements() {
        let mut zero = Slots::<(), 0>::default();
        assert_eq!(zero.push(()), Err(()));
        assert!(zero.as_slice().is_empty());
        let mut one = Slots::<(), 1>::default();
        one.push(()).unwrap();
        assert_eq!(one.as_slice().len(), 1);
        assert_eq!(one.push(()), Err(()));
    }
    #[test]
    fn rejected_popped_and_stored_values_each_drop_once() {
        struct Counter(Rc<Cell<usize>>);
        impl Drop for Counter {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        {
            let mut slots = Slots::<Counter, 2>::default();
            assert!(slots.push(Counter(drops.clone())).is_ok());
            assert!(slots.push(Counter(drops.clone())).is_ok());
            let rejected = slots.push(Counter(drops.clone())).err().unwrap();
            assert_eq!(drops.get(), 0);
            drop(rejected);
            drop(slots.pop());
            assert_eq!(drops.get(), 2);
        }
        assert_eq!(drops.get(), 3);
    }
}
