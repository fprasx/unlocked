use unlocked::secvec::SecVec;
use std::sync::Arc;
use std::thread;

fn main() {
    let sv = SecVec::<isize>::new();
    let data = Arc::new(sv);
    let data2 = Arc::clone(&data);
    let data3 = Arc::clone(&data);
    let t1 = thread::spawn(move || {
        for i in 0..100000 {
            data.push(i);
        }
    });
    let t2 = thread::spawn(move || {
        for i in 0..100000 {
            data2.push(i);
        }
    });
    let _ = t1.join().unwrap();
    let _ = t2.join().unwrap();
    assert_eq!(200000, data3.size());
}
