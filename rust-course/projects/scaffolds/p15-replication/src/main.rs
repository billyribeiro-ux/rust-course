// Learner checkpoint 0: append-only indexed history; add transport simulation next.
fn append(log:&mut Vec<i64>,index:usize,value:i64)->Result<(),&'static str>{
    if index==log.len(){log.push(value);return Ok(());}
    if log.get(index)==Some(&value){Ok(())}else{Err("gap or conflicting duplicate")}
}
fn main(){let mut log=Vec::new();append(&mut log,0,42).unwrap();println!("{log:?}");}
#[cfg(test)]mod tests{use super::*;#[test]fn duplicate_is_idempotent(){let mut log=vec![];append(&mut log,0,42).unwrap();append(&mut log,0,42).unwrap();assert_eq!(log,vec![42]);}}
