// INTENTIONAL COMPILE FAILURE: the method is generic, so this trait is not dyn-compatible.
trait Encode{fn encode<T>(&self,value:T);}
fn use_encoder(_: &dyn Encode){}
fn main(){}
