use std::io::{self, Write};

// Monetary amounts in this example are whole cents; fractions are not accepted.
fn subtotal_cents(unit_price_cents: u32, quantity: u32) -> Option<u32> {
    unit_price_cents.checked_mul(quantity)
}

fn delivery_fee_cents(quantity: u32) -> u32 {
    if quantity == 0 { 0 } else { 250 }
}

fn main() -> io::Result<()> {
    let mut output = io::stdout().lock();
    let unit_price_cents: u32 = 125;
    let quantity: u32 = 0b0011;
    writeln!(output, "quantity: {quantity} (binary {quantity:b})")?;

    let subtotal = match subtotal_cents(unit_price_cents, quantity) {
        Some(value) => value,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "subtotal does not fit in u32 cents",
            ));
        }
    };
    writeln!(output, "subtotal cents: {subtotal}")?;
    writeln!(output, "delivery cents: {}", delivery_fee_cents(quantity))?;

    for item_number in 1..=quantity {
        writeln!(output, "pack item {item_number}")?;
    }

    let mut remaining = 2_u32;
    while remaining > 0 {
        writeln!(output, "checks remaining: {remaining}")?;
        // The loop condition proves that subtracting one cannot underflow.
        remaining -= 1;
    }
    writeln!(output, "ready")?;
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::{delivery_fee_cents, subtotal_cents};

    #[test]
    fn subtotal_uses_whole_cents_and_handles_zero_quantity() {
        assert_eq!(subtotal_cents(125, 3), Some(375));
        assert_eq!(subtotal_cents(u32::MAX, 0), Some(0));
    }

    #[test]
    fn subtotal_rejects_overflow_without_wrapping() {
        assert_eq!(subtotal_cents(u32::MAX, 1), Some(u32::MAX));
        assert_eq!(subtotal_cents(u32::MAX, 2), None);
    }

    #[test]
    fn delivery_is_charged_only_for_nonempty_orders() {
        assert_eq!(delivery_fee_cents(0), 0);
        assert_eq!(delivery_fee_cents(1), 250);
        assert_eq!(delivery_fee_cents(u32::MAX), 250);
    }
}
