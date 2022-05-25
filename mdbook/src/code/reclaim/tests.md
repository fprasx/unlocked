# More tests

Here are the tests. I added a new one up at the top that spawns a bunch of
threads which `push` and `pop`. I just want to make sure `Miri` does not detect
any UB in a complex scenario like that

```rust
#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};
    use std::vec::Vec;

    #[test]
    fn the_big_multithread() {
        static FIVE: isize = 5;
        let data = Arc::new(SecVec::<isize>::new());
        data.reserve(100 * 5);
        let sum = Arc::new(AtomicIsize::new(0));
        #[allow(clippy::needless_collect)]
        let handles = (0..5)
            .map(|_| {
                let data = Arc::clone(&data);
                thread::spawn(move || {
                    for _ in 0..100 {
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
                    for _ in 0..100 {
                        sum.fetch_add(data.pop().unwrap_or(0), Ordering::Relaxed);
                    }
                })
            })
            .into_iter()
            .collect::<Vec<JoinHandle<_>>>();
        handles.into_iter().for_each(|h| h.join().unwrap());
    }

    #[test]
    fn size_starts_at_0() {
        let sv = SecVec::<usize>::new();
        assert_eq!(0, sv.size());
    }

    #[test]
    fn pop_empty_returns_none() {
        let sv = SecVec::<usize>::new();
        assert_eq!(sv.pop(), None);
    }

    #[test]
    fn ten_push_ten_pop() {
        let sv = SecVec::<isize>::new();
        for i in 0..10 {
            sv.push(i);
        }
        for i in (0..10).rev() {
            assert_eq!(sv.pop(), Some(i));
        }
    }

    #[test]
    fn does_not_allocate_buffers_on_new() {
        let sv = SecVec::<isize>::new();
        for buffer in &**sv.buffers {
            assert!(buffer.load(Ordering::Relaxed).is_null())
        }
    }
}

```

And here is the result, shuffled around a little so it all fits:

```zsh
~/C/r/unlocked (main) [1] > cargo miri test -- sealed::tests
    Finished test [unoptimized + debuginfo] target(s) in 0.01s
     Running unittests (target/miri/x86_64-apple-darwin/debug/deps/unlocked-666)

running 5 tests
test sealed::tests::does_not_allocate_buffers_on_new ... ok
test sealed::tests::pop_empty_returns_none ... ok
test sealed::tests::size_starts_at_0 ... ok
test sealed::tests::ten_push_ten_pop ... ok
test sealed::tests::the_big_multithread ... ok
    warning: thread support is experimental and incomplete:
        weak memory effects are not emulated.
```

Mmm . . . I love those greens! `<3`
