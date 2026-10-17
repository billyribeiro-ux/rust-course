use core_rust_labs::conversions::{
    Percent, PercentError, parse_percentages, selected_basis_points,
};

fn main() -> Result<(), PercentError> {
    let values = parse_percentages(&["0", "025", "100"])?;
    let threshold: Percent = 25u16.try_into()?;
    println!("threshold percent: {threshold}");
    println!(
        "selected basis points: {:?}",
        selected_basis_points(&values, threshold)
    );
    let numeric: u16 = threshold.into();
    println!("lossless numeric conversion: {numeric}");
    Ok(())
}
