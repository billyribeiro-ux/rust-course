#![no_std]
//! No allocator, OS, threads or I/O: a small portable data transform.
//! A library does not supply a panic handler; the final executable chooses one.
pub fn clamp_sample(value: u16, ceiling: u16) -> u16 { if value > ceiling { ceiling } else { value } }
#[cfg(test)]mod tests{#[test]fn limits(){assert_eq!(super::clamp_sample(1024,255),255);assert_eq!(super::clamp_sample(7,255),7);}}
