//! Deterministic fixed-leader replication simulator. NOT Raft; no leader election.
use std::collections::{BTreeSet, VecDeque};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Append { index: usize, value: i64 },
    Ack { index: usize },
}
#[derive(Clone, Debug)]
pub struct Packet {
    pub from: usize,
    pub to: usize,
    pub body: Message,
}
#[derive(Clone, Debug, Default)]
pub struct Node {
    pub durable: Vec<i64>,
    pub online: bool,
}
#[derive(Debug)]
pub struct Lab {
    pub nodes: [Node; 3],
    pub queue: VecDeque<Packet>,
    pub blocked: BTreeSet<(usize, usize)>,
    acks: Vec<BTreeSet<usize>>,
    pub committed: usize,
}
impl Default for Lab {
    fn default() -> Self {
        Self {
            nodes: std::array::from_fn(|_| Node {
                durable: vec![],
                online: true,
            }),
            queue: VecDeque::new(),
            blocked: BTreeSet::new(),
            acks: vec![],
            committed: 0,
        }
    }
}
impl Lab {
    pub fn propose(&mut self, value: i64) -> Result<usize, &'static str> {
        if !self.nodes[0].online {
            return Err("leader offline");
        }
        if self.nodes[0].durable.len() >= 1000 {
            return Err("log budget");
        }
        let index = self.nodes[0].durable.len();
        self.nodes[0].durable.push(value);
        self.acks.push(BTreeSet::from([0]));
        self.resend();
        Ok(index)
    }
    pub fn resend(&mut self) {
        self.queue.clear();
        for to in 1..3 {
            for (index, &value) in self.nodes[0].durable.iter().enumerate() {
                self.queue.push_back(Packet {
                    from: 0,
                    to,
                    body: Message::Append { index, value },
                });
            }
        }
    }
    pub fn partition(&mut self, a: usize, b: usize, on: bool) {
        for edge in [(a, b), (b, a)] {
            if on {
                self.blocked.insert(edge);
            } else {
                self.blocked.remove(&edge);
            }
        }
    }
    pub fn crash(&mut self, node: usize) {
        self.nodes[node].online = false;
    }
    pub fn restart(&mut self, node: usize) {
        self.nodes[node].online = true;
    }
    pub fn delay(&mut self) {
        if let Some(p) = self.queue.pop_front() {
            self.queue.push_back(p);
        }
    }
    pub fn drop_next(&mut self) {
        self.queue.pop_front();
    }
    pub fn step(&mut self) -> bool {
        let Some(packet) = self.queue.pop_front() else {
            return false;
        };
        if !self.nodes[packet.to].online || self.blocked.contains(&(packet.from, packet.to)) {
            return true;
        }
        match packet.body {
            Message::Append { index, value } => {
                if packet.from != 0 || packet.to == 0 {
                    return true;
                }
                let log = &mut self.nodes[packet.to].durable;
                if index == log.len() {
                    log.push(value);
                }
                if log.get(index) == Some(&value) {
                    self.queue.push_back(Packet {
                        from: packet.to,
                        to: 0,
                        body: Message::Ack { index },
                    });
                }
            }
            Message::Ack { index } => {
                if packet.to != 0 || packet.from == 0 {
                    return true;
                }
                if let Some(acks) = self.acks.get_mut(index) {
                    acks.insert(packet.from);
                }
                while self.committed < self.acks.len() && self.acks[self.committed].len() >= 2 {
                    self.committed += 1;
                }
            }
        }
        true
    }
    pub fn drain(&mut self) {
        while self.step() {}
    }
    pub fn prefix_safe(&self) -> bool {
        self.nodes
            .iter()
            .all(|node| self.nodes[0].durable.starts_with(&node.durable))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partition_blocks_commit_until_heal() {
        let mut l = Lab::default();
        l.partition(0, 1, true);
        l.partition(0, 2, true);
        l.propose(7).unwrap();
        l.drain();
        assert_eq!(l.committed, 0);
        assert!(l.prefix_safe());
        l.partition(0, 1, false);
        l.resend();
        l.drain();
        assert_eq!(l.committed, 1);
    }
    #[test]
    fn duplicates_restarts_delay_and_loss() {
        for dropped in 0..12 {
            let mut l = Lab::default();
            l.propose(10).unwrap();
            l.propose(20).unwrap();
            for step in 0..12 {
                if step == dropped {
                    l.drop_next();
                }
                if step % 3 == 0 {
                    l.delay();
                }
                l.step();
                assert!(l.prefix_safe());
            }
            l.crash(1);
            l.resend();
            l.drain();
            l.restart(1);
            l.resend();
            l.drain();
            assert!(l.prefix_safe());
            assert_eq!(l.nodes[1].durable, vec![10, 20]);
            assert_eq!(l.committed, 2);
        }
    }
    #[test]
    fn conflicting_append_cannot_replace_prefix() {
        let mut l = Lab::default();
        l.propose(1).unwrap();
        l.drain();
        l.queue.push_back(Packet {
            from: 0,
            to: 1,
            body: Message::Append {
                index: 0,
                value: 999,
            },
        });
        l.drain();
        assert_eq!(l.nodes[1].durable, vec![1]);
    }
    #[test]
    fn later_and_duplicate_acknowledgments_do_not_skip_a_commit_hole() {
        let mut l = Lab::default();
        l.propose(10).unwrap();
        l.propose(20).unwrap();
        // Deliver the two legitimate appends to follower 1, but neither ack yet.
        assert!(l.step());
        assert!(l.step());
        assert_eq!(l.nodes[1].durable, [10, 20]);
        assert_eq!(l.committed, 0);
        let later = l
            .queue
            .iter()
            .position(|packet| packet.from == 1 && packet.body == Message::Ack { index: 1 })
            .unwrap();
        let ack = l.queue.remove(later).unwrap();
        for _ in 0..3 {
            l.queue.push_front(ack.clone());
            assert!(l.step());
            assert_eq!(l.committed, 0);
            assert_eq!(l.acks[1], BTreeSet::from([0, 1]));
            assert!(l.prefix_safe());
        }
        let earlier = l
            .queue
            .iter()
            .position(|packet| packet.from == 1 && packet.body == Message::Ack { index: 0 })
            .unwrap();
        let ack = l.queue.remove(earlier).unwrap();
        l.queue.push_front(ack);
        assert!(l.step());
        assert_eq!(l.committed, 2);
        assert!(l.nodes[2].durable.is_empty()); // Only leader plus one follower.
        assert!(l.prefix_safe());
    }
}
