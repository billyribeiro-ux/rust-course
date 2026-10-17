fn main() {
    let mut slots = checkpoint_p13_safe_slots::Slots::<&str, 1>::default();
    match slots.push("first") {
        Ok(()) => println!("stored first"),
        Err(value) => println!("capacity rejected {value}"),
    }
    if let Err(value) = slots.push("second") {
        println!("returned {value}");
    }
    println!("popped={:?}", slots.pop());
}
