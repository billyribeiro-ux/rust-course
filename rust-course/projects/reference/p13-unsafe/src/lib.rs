//! Tiny fixed-capacity stack for auditing unsafe boundaries, not a faster-Vec claim.
#![deny(unsafe_op_in_unsafe_fn)]
use std::mem::MaybeUninit;

/// Invariant: len <= N; exactly slots[0..len] contain live T values.
/// Safe callers cannot alter slots or len directly. No references survive a mutation
/// because all mutation requires &mut self. Uninitialized storage is never exposed as T.
pub struct FixedSlots<T, const N: usize> {
    slots: [MaybeUninit<T>; N],
    len: usize,
}
impl<T, const N: usize> Default for FixedSlots<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T, const N: usize> FixedSlots<T, N> {
    pub fn new() -> Self {
        Self {
            slots: [const { MaybeUninit::uninit() }; N],
            len: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, value: T) -> Result<(), T> {
        if self.len == N {
            return Err(value);
        }
        self.slots[self.len].write(value); // Initialize before publishing the new length.
        self.len += 1;
        Ok(())
    }
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1; // Remove from live prefix before transferring ownership.
        // SAFETY: old len was positive, so this slot was initialized. Shrinking len
        // ensures this value is moved out exactly once and never dropped by self.
        Some(unsafe { self.slots[self.len].assume_init_read() })
    }
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: MaybeUninit<T> has T's size/alignment. The prefix is initialized,
        // within this one array allocation, and borrowed for exactly self's lifetime.
        // Array pointers are non-null/aligned even for N=0 or a zero-sized T.
        unsafe { std::slice::from_raw_parts(self.slots.as_ptr().cast::<T>(), self.len) }
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY: same layout/initialization/range reasoning as as_slice; &mut self
        // gives exclusive access for the returned slice's lifetime, including ZSTs.
        unsafe { std::slice::from_raw_parts_mut(self.slots.as_mut_ptr().cast::<T>(), self.len) }
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.as_slice().get(index)
    }
    pub fn clear(&mut self) {
        let len = std::mem::take(&mut self.len); // Publish empty state before user Drop code.
        // SAFETY: old prefix contained len initialized owned Ts. The mutable borrow
        // excludes references. Slice drop glue drops each element, including cleanup
        // during a single destructor panic; len=0 prevents a later double drop.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.slots.as_mut_ptr().cast::<T>(),
                len,
            ));
        }
    }
}
impl<T, const N: usize> Drop for FixedSlots<T, N> {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Safe comparison implementation: same capacity policy, heap-backed storage.
pub struct SafeSlots<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for SafeSlots<T, N> {
    fn default() -> Self {
        Self(Vec::with_capacity(N))
    }
}
impl<T, const N: usize> SafeSlots<T, N> {
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
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };
    #[test]
    fn empty_zero_capacity_and_bounds() {
        let mut zero = FixedSlots::<String, 0>::new();
        assert_eq!(zero.push("owned".into()), Err("owned".into()));
        assert_eq!(zero.pop(), None);
        assert_eq!(zero.get(0), None);
        assert!(zero.as_mut_slice().is_empty());
        let mut one = FixedSlots::<u32, 1>::new();
        one.push(7).unwrap();
        assert_eq!(one.get(1), None);
        assert_eq!(one.push(8), Err(8));
        assert_eq!(one.pop(), Some(7));
        assert_eq!(one.pop(), None);
    }
    #[test]
    fn agrees_with_safe_baseline_under_many_operations() {
        let mut actual = FixedSlots::<u64, 7>::new();
        let mut expected = SafeSlots::<u64, 7>::default();
        let mut seed = 19u64;
        for _ in 0..10_000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            if seed & 3 == 0 {
                assert_eq!(actual.pop(), expected.pop());
            } else {
                assert_eq!(actual.push(seed), expected.push(seed));
            }
            assert_eq!(actual.as_slice(), expected.as_slice());
        }
    }
    #[test]
    fn values_are_dropped_exactly_once_and_rejected_value_is_returned() {
        struct Count(Rc<Cell<usize>>);
        impl Drop for Count {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let count = Rc::new(Cell::new(0));
        {
            let mut slots = FixedSlots::<Count, 2>::new();
            assert!(slots.push(Count(count.clone())).is_ok());
            assert!(slots.push(Count(count.clone())).is_ok());
            let rejected = slots.push(Count(count.clone())).err().unwrap();
            assert_eq!(count.get(), 0);
            drop(rejected);
            drop(slots.pop());
            assert_eq!(count.get(), 2);
        }
        assert_eq!(count.get(), 3);
    }
    #[test]
    fn zero_sized_values_are_logical_elements() {
        let mut slots = FixedSlots::<(), 3>::new();
        for _ in 0..3 {
            slots.push(()).unwrap();
        }
        assert_eq!(slots.as_slice().len(), 3);
        assert_eq!(slots.push(()), Err(()));
        assert_eq!(slots.pop(), Some(()));
        slots.clear();
        assert!(slots.is_empty());
    }
    #[test]
    fn zero_sized_destructors_run_for_every_element() {
        thread_local! { static DROPS: Cell<usize> = const { Cell::new(0) }; }
        struct Z;
        impl Drop for Z {
            fn drop(&mut self) {
                DROPS.with(|n| n.set(n.get() + 1));
            }
        }
        DROPS.with(|n| n.set(0));
        {
            let mut slots = FixedSlots::<Z, 3>::new();
            for _ in 0..3 {
                assert!(slots.push(Z).is_ok());
            }
        }
        DROPS.with(|n| assert_eq!(n.get(), 3));
    }
    #[test]
    fn alignment_is_preserved() {
        #[repr(align(64))]
        struct Aligned(u8);
        let mut slots = FixedSlots::<Aligned, 2>::new();
        assert!(slots.push(Aligned(4)).is_ok());
        assert_eq!(slots.as_slice().as_ptr() as usize % 64, 0);
        slots.as_mut_slice()[0].0 = 9;
        assert_eq!(slots.get(0).unwrap().0, 9);
    }
    #[test]
    fn a_single_destructor_panic_leaves_empty_reusable_storage() {
        struct Bomb {
            drops: Rc<Cell<usize>>,
            explode: bool,
        }
        impl Drop for Bomb {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
                assert!(!self.explode, "injected destructor panic");
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut slots = FixedSlots::<Bomb, 3>::new();
        for explode in [false, true, false] {
            assert!(
                slots
                    .push(Bomb {
                        drops: drops.clone(),
                        explode
                    })
                    .is_ok()
            );
        }
        assert!(catch_unwind(AssertUnwindSafe(|| slots.clear())).is_err());
        assert_eq!(drops.get(), 3);
        assert!(slots.is_empty());
        assert!(
            slots
                .push(Bomb {
                    drops: drops.clone(),
                    explode: false
                })
                .is_ok()
        );
        drop(slots);
        assert_eq!(drops.get(), 4);
    }
}
