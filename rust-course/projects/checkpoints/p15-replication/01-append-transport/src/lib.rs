//! Fixed-leader append transport model. No acknowledgments or committed prefix yet.
//! Packet scheduling is explicit data, not a real network or consensus algorithm.
use std::collections::VecDeque;
#[derive(Debug, Clone, Copy)]
pub struct Packet {
    pub to: usize,
    pub index: usize,
    pub value: i64,
}
#[derive(Default)]
pub struct Lab {
    logs: [Vec<i64>; 3],
    pending: VecDeque<Packet>,
}
impl Lab {
    pub fn propose(&mut self, value: i64) -> Result<(), &'static str> {
        if self.logs[0].len() >= 8 || self.pending.len() > 14 {
            return Err("budget");
        }
        let index = self.logs[0].len();
        self.logs[0].push(value);
        for to in [1, 2] {
            self.pending.push_back(Packet { to, index, value });
        }
        Ok(())
    }
    pub fn take(&mut self) -> Option<Packet> {
        self.pending.pop_front()
    }
    pub fn deliver(&mut self, packet: Packet) -> Result<bool, &'static str> {
        if !(1..=2).contains(&packet.to) {
            return Err("invalid follower");
        }
        let log = &mut self.logs[packet.to];
        if packet.index == log.len() {
            if log.len() >= 8 {
                return Err("log budget");
            }
            log.push(packet.value);
            return Ok(true);
        }
        if log.get(packet.index) == Some(&packet.value) {
            Ok(false)
        } else {
            Err("gap or conflict")
        }
    }
    pub fn log(&self, node: usize) -> Option<&[i64]> {
        self.logs.get(node).map(Vec::as_slice)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_delivery_has_one_effect() {
        let mut l = Lab::default();
        l.propose(42).unwrap();
        let packet = l.take().unwrap();
        assert!(l.deliver(packet).unwrap());
        assert!(!l.deliver(packet).unwrap());
        assert_eq!(l.log(1), Some(&[42][..]));
    }
    #[test]
    fn delay_can_create_a_gap_without_corrupting_prefix() {
        let mut l = Lab::default();
        l.propose(10).unwrap();
        l.propose(20).unwrap();
        let first = l.take().unwrap();
        let _other = l.take().unwrap();
        let later = l.take().unwrap();
        assert!(l.deliver(later).is_err());
        assert_eq!(l.log(1), Some(&[][..]));
        l.deliver(first).unwrap();
        l.deliver(later).unwrap();
        assert_eq!(l.log(1), Some(&[10, 20][..]));
    }
    #[test]
    fn invalid_target_and_capacity_are_explicit() {
        let mut l = Lab::default();
        assert!(
            l.deliver(Packet {
                to: 99,
                index: 0,
                value: 3
            })
            .is_err()
        );
        for n in 0..8 {
            l.propose(n).unwrap();
        }
        assert!(l.propose(9).is_err());
        assert_eq!(l.log(0).unwrap().len(), 8);
    }
}
