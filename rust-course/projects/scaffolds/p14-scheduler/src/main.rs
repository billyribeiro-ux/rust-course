// Learner checkpoint 0: state transitions; persistence and leases come later.
#[derive(Debug,PartialEq,Eq)] enum State {Queued,Running,Done}
fn start(state:&mut State)->Result<(),&'static str>{if *state!=State::Queued{return Err("not queued");}*state=State::Running;Ok(())}
fn finish(state:&mut State)->Result<(),&'static str>{if *state!=State::Running{return Err("not running");}*state=State::Done;Ok(())}
fn main(){let mut state=State::Queued;start(&mut state).unwrap();finish(&mut state).unwrap();println!("{state:?}");}
#[cfg(test)]mod tests{use super::*;#[test]fn cannot_skip_running(){let mut s=State::Queued;assert!(finish(&mut s).is_err());assert_eq!(s,State::Queued);}}
