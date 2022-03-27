use std::sync::Arc;
use std::thread::{self, JoinHandle};
use unlocked::leaky::SecVec;
fn main() {
    let sv = Arc::new(SecVec::<isize>::new());
    #[allow(clippy::needless_collect)]
    let handles = (0..20)
        .map(|val| {
            let data = Arc::clone(&sv);
            if val % 2 == 0 {
                thread::spawn(move || {
                    for i in 0..100000 {
                        data.push(i);
                    }
                })
            } else {
                thread::spawn(move || {
                    for _ in 0..100000 {
                        data.pop();
                    }
                })
            }
        })
        .collect::<Vec<JoinHandle<()>>>();
    handles.into_iter().for_each(|h| h.join().unwrap());
    println!("{}", sv.size());
}
