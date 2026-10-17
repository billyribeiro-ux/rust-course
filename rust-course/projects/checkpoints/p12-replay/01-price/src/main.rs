fn main() -> Result<(), checkpoint_p12_price::InvalidPrice> {
    let price = checkpoint_p12_price::Price::parse("12.34")?;
    println!("price={price} ticks={}", price.ticks());
    Ok(())
}
