// Learner checkpoint 0: all trading is simulated, fixed-point integer cents.
fn buy(cash:i64,price_cents:i64,quantity:i64)->Result<i64,&'static str>{
 if cash<0||price_cents<=0||quantity<=0{return Err("invalid input");}
 let cost=price_cents.checked_mul(quantity).ok_or("overflow")?;
 cash.checked_sub(cost).filter(|remaining|*remaining>=0).ok_or("insufficient cash")
}
fn main(){println!("simulated cash: {:?}",buy(1_000_000,125,3));}
#[cfg(test)]mod tests{use super::*;#[test]fn bounded_purchase(){assert_eq!(buy(100,30,3),Ok(10));assert!(buy(100,30,4).is_err());assert!(buy(100,i64::MAX,2).is_err());}}
