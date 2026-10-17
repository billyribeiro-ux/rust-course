//! Focused specialization examples. None is required to write every Rust application.
use std::ffi::c_int;
pub trait KindName {
    fn kind_name() -> &'static str;
}
#[derive(course_kind_derive::KindName)]
pub struct Packet<T, const N: usize> {
    pub values: [T; N],
}

/// A higher-ranked bound: the callback works for every input lifetime.
/// ```
/// use rust_specializations::with_first;
/// assert_eq!(with_first("alpha beta", |s|s.split_whitespace().next().unwrap_or("")), "alpha");
/// ```
pub fn with_first<'a, F>(input: &'a str, select: F) -> &'a str
where
    F: for<'b> Fn(&'b str) -> &'b str,
{
    select(input)
}

/// The associated output is fixed by each implementor.
pub trait Source {
    type Item;
    fn next_item(&mut self) -> Option<Self::Item>;
}
pub struct Countdown(pub u8);
impl Source for Countdown {
    type Item = u8;
    fn next_item(&mut self) -> Option<u8> {
        let value = self.0.checked_sub(1)?;
        self.0 = value;
        Some(value)
    }
}
pub fn drain(source: &mut dyn Source<Item = u8>) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(item) = source.next_item() {
        out.push(item);
    }
    out
}

/// Each supplied expression is evaluated exactly once, left to right.
#[macro_export]
macro_rules! checked_sum {
    ($($value:expr),* $(,)?) => {{
        let mut total=Some(0i64);
        $(let next: i64=$value; total=total.and_then(|old|old.checked_add(next));)*
        total
    }};
}

unsafe extern "C" {
    fn abs(value: c_int) -> c_int;
}
/// A narrow C ABI example. Prefer `i32::checked_abs` in ordinary Rust code.
/// Returns None for the only input whose C absolute value is not representable.
pub fn c_checked_abs(value: c_int) -> Option<c_int> {
    if value == c_int::MIN {
        return None;
    }
    // SAFETY: abs has the declared C ABI, has no pointer/lifetime contract,
    // and its result is representable because INT_MIN was excluded above.
    Some(unsafe { abs(value) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generic_derive_retains_parameters() {
        assert_eq!(Packet::<u8, 4>::kind_name(), "Packet");
        let p = Packet { values: [1; 0] };
        assert!(p.values.is_empty());
    }
    #[test]
    fn trait_object_associated_item() {
        assert_eq!(drain(&mut Countdown(3)), [2, 1, 0]);
    }
    #[test]
    fn macro_once_and_overflow() {
        let mut n = 0;
        assert_eq!(
            checked_sum!(
                {
                    n += 1;
                    n
                },
                {
                    n += 1;
                    n
                }
            ),
            Some(3)
        );
        assert_eq!(n, 2);
        assert_eq!(checked_sum!(i64::MAX, 1), None);
    }
    #[test]
    fn ffi_boundary() {
        for x in [-2, 0, 1, i32::MAX, i32::MIN] {
            assert_eq!(c_checked_abs(x), x.checked_abs());
        }
    }
}
