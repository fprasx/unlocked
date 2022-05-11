use unlocked::sealed::SecVec;
fn main() {
    let sv = SecVec::<isize>::new();
    for i in 1..11 {
        sv.push(i);
    }
    for _ in 1..11 {
        sv.pop();
    }
}
