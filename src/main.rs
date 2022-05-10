use unlocked::sealed::SecVec;
fn main() {
    let sv = SecVec::<isize>::new();
    for _ in 0..100 {
        sv.push(1);
    }
    println!("{sv:?}")
}
