use std::mem::MaybeUninit;
fn main() {
    let slot = MaybeUninit::new(7u32);
    let _value = slot.assume_init_read();
}
