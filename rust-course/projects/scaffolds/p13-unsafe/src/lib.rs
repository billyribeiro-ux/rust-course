#![forbid(unsafe_code)]
/// Establish the safe behavior before replacing any implementation with unsafe code.
pub struct Slots<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Slots<T, N> { fn default() -> Self { Self(Vec::with_capacity(N)) } }
impl<T, const N: usize> Slots<T, N> {
    pub fn push(&mut self, _value: T) -> Result<(), T> { todo!("Learner: preserve ownership on capacity failure") }
    pub fn pop(&mut self) -> Option<T> { todo!("Learner: transfer exactly one owned value") }
    pub fn as_slice(&self) -> &[T] { &self.0 }
}
#[cfg(test)] mod acceptance {
    use super::*;
    #[test] fn capacity_and_ownership() { let mut s = Slots::<String, 1>::default(); s.push("first".into()).unwrap(); assert_eq!(s.push("second".into()), Err("second".into())); assert_eq!(s.pop(), Some("first".into())); assert_eq!(s.pop(), None); }
    #[test] fn zero_capacity_and_zst() { let mut z = Slots::<(), 0>::default(); assert_eq!(z.push(()), Err(())); assert!(z.as_slice().is_empty()); let mut one = Slots::<(), 1>::default(); one.push(()).unwrap(); assert_eq!(one.as_slice().len(), 1); }
}
