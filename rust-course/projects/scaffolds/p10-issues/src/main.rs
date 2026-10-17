// Learner checkpoint 0: validate a title before adding HTTP or storage.
fn title(input: &str) -> Result<&str, &'static str> {
    let input=input.trim();
    if input.is_empty() || input.chars().count()>200 {Err("invalid title")} else {Ok(input)}
}
fn main(){println!("{:?}",title("  investigate timeout  "));}
#[cfg(test)]mod tests{use super::*;#[test]fn invalid_title(){assert!(title(" ").is_err());assert_eq!(title(" fix "),Ok("fix"));}}
