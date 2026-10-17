// INTENTIONAL COMPILE FAILURE: neither trait nor type belongs to this crate.
impl std::fmt::Display for Vec<u8> {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"bytes")}
}
fn main(){}
