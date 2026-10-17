//! Synthetic single-symbol, in-memory order domain checkpoint. No market access.
//! Integer cents and whole shares only; no persistence, jobs, chart, or HTTP yet.
use std::collections::BTreeMap;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Accepted {
    side: Side,
    quantity: i64,
    price: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Engine {
    cash: i64,
    shares: i64,
    quote: Option<(u64, i64)>,
    orders: BTreeMap<String, Accepted>,
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            cash: 1_000_000,
            shares: 0,
            quote: None,
            orders: BTreeMap::new(),
        }
    }
}
impl Engine {
    pub fn tick(&mut self, sequence: u64, price: i64) -> Result<(), &'static str> {
        if price <= 0 || self.quote.is_some_and(|(old, _)| sequence <= old) {
            return Err("invalid or out-of-order tick");
        }
        self.quote = Some((sequence, price));
        Ok(())
    }
    pub fn order(&mut self, id: &str, side: Side, quantity: i64) -> Result<bool, &'static str> {
        if id.is_empty() || id.len() > 64 || quantity <= 0 {
            return Err("invalid order");
        }
        if let Some(old) = self.orders.get(id) {
            return if old.side == side && old.quantity == quantity {
                Ok(false)
            } else {
                Err("identity conflict")
            };
        }
        if self.orders.len() >= 64 {
            return Err("retained order budget");
        }
        let price = self.quote.ok_or("missing quote")?.1;
        let cost = price.checked_mul(quantity).ok_or("cost overflow")?;
        let (cash, shares) = match side {
            Side::Buy => (
                self.cash
                    .checked_sub(cost)
                    .filter(|v| *v >= 0)
                    .ok_or("insufficient cash")?,
                self.shares
                    .checked_add(quantity)
                    .ok_or("position overflow")?,
            ),
            Side::Sell => (
                self.cash.checked_add(cost).ok_or("cash overflow")?,
                self.shares
                    .checked_sub(quantity)
                    .filter(|v| *v >= 0)
                    .ok_or("insufficient shares")?,
            ),
        };
        // All fallible domain checks precede this mutation boundary.
        self.orders.insert(
            id.into(),
            Accepted {
                side,
                quantity,
                price,
            },
        );
        self.cash = cash;
        self.shares = shares;
        Ok(true)
    }
    pub fn balances(&self) -> (i64, i64) {
        (self.cash, self.shares)
    }
    pub fn accepted_price(&self, id: &str) -> Option<i64> {
        self.orders.get(id).map(|a| a.price)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_preserves_original_fill_after_quote_changes() {
        let mut e = Engine::default();
        e.tick(1, 125).unwrap();
        assert!(e.order("a", Side::Buy, 3).unwrap());
        e.tick(2, 200).unwrap();
        let before = e.clone();
        assert!(!e.order("a", Side::Buy, 3).unwrap());
        assert_eq!(e, before);
        assert_eq!(e.accepted_price("a"), Some(125));
        assert!(e.order("a", Side::Buy, 4).is_err());
    }
    #[test]
    fn rejection_has_no_partial_ledger_effect() {
        let mut e = Engine::default();
        assert!(e.order("a", Side::Buy, 1).is_err());
        e.tick(1, i64::MAX).unwrap();
        let before = e.clone();
        assert!(e.order("a", Side::Buy, 2).is_err());
        assert!(e.order("b", Side::Sell, 1).is_err());
        assert_eq!(e, before);
        assert!(e.tick(1, 10).is_err());
        assert_eq!(e, before);
    }
    #[test]
    fn integer_purchase_and_sale_balance() {
        let mut e = Engine::default();
        e.tick(1, 125).unwrap();
        e.order("buy", Side::Buy, 3).unwrap();
        e.order("sell", Side::Sell, 2).unwrap();
        assert_eq!(e.balances(), (999875, 1));
    }
    #[test]
    fn admission_is_bounded_but_exact_retries_still_work() {
        let mut e = Engine::default();
        e.tick(1, 1).unwrap();
        for n in 0..64 {
            e.order(&format!("o{n}"), Side::Buy, 1).unwrap();
        }
        let before = e.clone();
        assert!(e.order("overflow", Side::Buy, 1).is_err());
        assert!(!e.order("o0", Side::Buy, 1).unwrap());
        assert_eq!(e, before);
    }
}
