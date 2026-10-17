fn main() { let mut tasks = vec![1]; let a = &mut tasks; let b = &mut tasks; a.push(2); b.push(3); }
