use std::sync::Arc;
use std::thread::{self, JoinHandle};
use unlocked::leaky::SecVec;
fn main() {
    let sv = Arc::new(SecVec::<isize>::new());
    #[allow(clippy::needless_collect)]
    let handles = (0..10)
        .map(|_| {
            let data = Arc::clone(&sv);
            thread::spawn(move || {
                for i in 0..100000 {
                    data.push(i);
                }
            })
        })
        .collect::<Vec<JoinHandle<()>>>();
    handles.into_iter().for_each(|h| h.join().unwrap());
    assert_eq!(sv.size(), 10 * 100000);
}
