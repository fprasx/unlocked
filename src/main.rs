extern crate std;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::vec::Vec;
use unlocked::sealed::SecVec;

fn main() {
    static FIVE: isize = 5;
    let data = Arc::new(SecVec::<isize>::new());
    data.reserve(100 * 5);
    let sum = Arc::new(AtomicIsize::new(0));
    #[allow(clippy::needless_collect)]
    let handles = (0..5)
        .map(|_| {
            let data = Arc::clone(&data);
            thread::spawn(move || {
                for _ in 0..1000 {
                    data.push(FIVE);
                }
            })
        })
        .into_iter()
        .collect::<Vec<JoinHandle<_>>>();
    handles.into_iter().for_each(|h| h.join().unwrap());
    #[allow(clippy::needless_collect)]
    let handles = (0..5)
        .map(|_| {
            let data = Arc::clone(&data);
            let sum = Arc::clone(&sum);
            thread::spawn(move || {
                for _ in 0..1000 {
                    sum.fetch_add(data.pop().unwrap_or(0), Ordering::Relaxed);
                }
            })
        })
        .into_iter()
        .collect::<Vec<JoinHandle<_>>>();
    handles.into_iter().for_each(|h| h.join().unwrap());
}
