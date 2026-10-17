// INTENTIONAL COMPILE FAILURE: E0277, PhantomPinned prevents automatic Unpin.
use std::{marker::PhantomPinned, pin::Pin};
struct Sticky {
    value: u8,
    _pin: PhantomPinned,
}
fn main() {
    let mut pinned = Box::pin(Sticky {
        value: 1,
        _pin: PhantomPinned,
    });
    let reference: Pin<&mut Sticky> = pinned.as_mut();
    reference.get_mut().value = 2;
}
