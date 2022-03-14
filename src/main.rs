use lfvec::secvec;
fn main() {
    let sv = secvec::SecVec::<isize>::new();
    sv.push(-69);
    println!("{:?}", sv.pop());
}
