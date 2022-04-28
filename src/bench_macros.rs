#[macro_export]
macro_rules! mutex_vec_of_ref {
    ($num_iters:expr, $($bench_name:ident:$num_threads:expr),*) => {
        $(
            #[bench]
            fn $bench_name(b: &mut test::Bencher) {
                use std::sync::atomic::{AtomicIsize, Ordering};
                use std::sync::{Mutex, Arc};
                use std::thread::{self, JoinHandle};
                use std::vec::Vec;
                let data = Arc::new(Mutex::new(Vec::<&isize>::with_capacity(
                    $num_iters * $num_threads,
                )));
                let sum = Arc::new(AtomicIsize::new(0));
                let var: &'static isize = test::black_box(&5);
                b.iter(|| {
                    // Create num_threads threads which will push a reference to leaked num_iters times
                    #[allow(clippy::needless_collect)]
                    let handles = (0..$num_threads)
                        .map(|_| {
                            let data = Arc::clone(&data);
                            thread::spawn(move || {
                                // Do the pushing
                                for _ in 0..$num_iters {
                                    let mut guard = data.lock().unwrap();
                                    guard.push(var);
                                }
                            })
                        })
                        .into_iter()
                        .collect::<Vec<JoinHandle<_>>>();
                    handles.into_iter().for_each(|h| h.join().unwrap());
                    // Create num_threads threads which will pop num_iters times
                    #[allow(clippy::needless_collect)]
                    let handles = (0..$num_threads)
                        .map(|_| {
                            let data = Arc::clone(&data);
                            let sum = Arc::clone(&sum);
                            thread::spawn(move || {
                                // Do the pushing
                                for _ in 0..$num_iters {
                                    let mut guard = data.lock().unwrap();
                                    sum.fetch_add(*guard.pop().unwrap(), Ordering::Relaxed);
                                }
                            })
                        })
                        .into_iter()
                        .collect::<Vec<JoinHandle<_>>>();
                    handles.into_iter().for_each(|h| h.join().unwrap());
                });
            }
        )*
    };
}

// I think I need to put this because the macros are only used in tests
#[allow(unused_imports)]
pub(crate) use mutex_vec_of_ref;

#[macro_export]
macro_rules! unlocked {
    ($num_iters:expr, $($bench_name:ident:$num_threads:expr),*) => {
        $(
            #[bench]
            fn $bench_name(b: &mut test::Bencher) {
                use std::sync::atomic::{AtomicIsize, Ordering};
                use std::sync::Arc;
                use std::thread::{self, JoinHandle};
                use std::vec::Vec;
                static FIVE: isize = 5;
                let data = Arc::new(SecVec::<isize>::new());
                data.reserve($num_iters * $num_threads);
                let sum = Arc::new(AtomicIsize::new(0));
                b.iter(|| {
                    // Create num_threads threads which will push a reference to leaked num_iters times
                    #[allow(clippy::needless_collect)]
                    let handles = (0..$num_threads)
                        .map(|_| {
                            let data = Arc::clone(&data);
                            thread::spawn(move || {
                                // Do the pushing
                                for _ in 0..$num_iters {
                                    data.push(FIVE);
                                }
                            })
                        })
                        .into_iter()
                        .collect::<Vec<JoinHandle<_>>>();
                    handles.into_iter().for_each(|h| h.join().unwrap());
                    // Create num_threads threads which will pop num_iters times
                    #[allow(clippy::needless_collect)]
                    let handles = (0..$num_threads)
                        .map(|_| {
                            let data = Arc::clone(&data);
                            let sum = Arc::clone(&sum);
                            thread::spawn(move || {
                                // Do the pushing
                                for _ in 0..$num_iters {
                                    sum.fetch_add(data.pop().unwrap(), Ordering::Relaxed);
                                }
                            })
                        })
                        .into_iter()
                        .collect::<Vec<JoinHandle<_>>>();
                    handles.into_iter().for_each(|h| h.join().unwrap());
                });
            }
        )*
    };
}

// I think I need to put this because the macros are only used in tests
#[allow(unused_imports)]
pub(crate) use unlocked;
