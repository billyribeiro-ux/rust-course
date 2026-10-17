use core_rust_labs::collections::{LabelError, count_labels, sample_bounds};

fn main() -> Result<(), LabelError> {
    let fixed = [12, -8, 15, 9];
    let mut variable = vec![12, -8];
    variable.push(15);
    variable.push(9);
    let marker: char = 'é';
    let same_length: bool = fixed.len() == variable.len();
    println!(
        "same length: {same_length}; marker UTF-8 bytes: {}",
        marker.len_utf8()
    );
    println!("array bounds: {:?}", sample_bounds(&fixed));
    println!("vector bounds: {:?}", sample_bounds(&variable));
    for (label, count) in count_labels(&["worker", "api", "api"])? {
        println!("{label}: {count}");
    }
    Ok(())
}
