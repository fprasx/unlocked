use lfvec::secvec;

fn main() {
    let sv = secvec::SecVec::<isize>::new();
    sv.reserve(25);
    for i in 0..25 {
        sv.push(i);
    }
    for _ in 0..25 {
        println!("Popped element {:?} off!", sv.pop());
    }
}
