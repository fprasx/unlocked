use lfvec::secvec;

fn main() {
    let sv = secvec::SecVec::<isize>::new();
    for i in 0..25 {
        sv.push(i);
    }
    let size = sv.size();
    for _ in 0..size {
        println!("{:?}", sv.pop());
    }
}
