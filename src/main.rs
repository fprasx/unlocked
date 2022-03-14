use lfvec::secvec;
use std::sync::atomic::Ordering;
// Problem in reserve
fn main() {
    let sv = secvec::SecVec::<isize>::new();
    sv.push(-69);
    println!("{:?}", sv.pop());
    // n_test(9)
}

fn n_test(rounds: usize) {
    let sv = secvec::SecVec::<isize>::new();
    sv.reserve(rounds);
    for _ in 0..rounds {
        sv.push(-69);
    }
    for _ in 0..rounds {
        assert!(sv.pop().is_some())
    }
}
