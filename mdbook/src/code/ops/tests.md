# Tests

Just for fun, I wrote some tests, and we get to satisfyingly see them pass.

```rust
#[cfg(test)]
mod tests {
    use super::*;

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

`Cargo` is super nice and we can use it to test. Running `cargo test` produces
the following output:

```
~/C/r/unlocked (main) > cargo test -- leaky::tests
    Finished test [unoptimized + debuginfo] target(s) in 0.01s
     Running unittests (target/debug/deps/unlocked-e6f64e7ba9c7e004)

running 4 tests
test leaky::tests::size_starts_at_0 ... ok
test leaky::tests::pop_empty_returns_none ... ok
test leaky::tests::does_not_allocate_buffers_on_new ... ok
test leaky::tests::ten_push_ten_pop ... ok
```

Although you can't see it, the green on those "ok"s warms my heart.

We know the vector is leaky, but otherwise it shouldn't be doing any other funky
things or UB. Let's see if `Miri` finds anything with
`MIRIFLAGS=-Zmiri-ignore-leaks cargo miri test -- leaky::tests`:

```
~/C/r/unlocked (main) > MIRIFLAGS=-Zmiri-ignore-leaks cargo miri test -- leaky::tests
    Finished test [unoptimized + debuginfo] target(s) in 0.01s
     Running unittests (target/miri/x86_64-apple-darwin/debug/deps/unlocked-4269)

running 4 tests
test leaky::tests::does_not_allocate_buffers_on_new ... ok
test leaky::tests::pop_empty_returns_none ... ok
test leaky::tests::size_starts_at_0 ... ok
test leaky::tests::ten_push_ten_pop ... ok
```

Nothing? Awesome! Just because `Miri` doesn't find anything doesn't mean nothing
fishy is happening. `Miri` combined with the rigorous analysis of the code we
did though is a very good sign.
