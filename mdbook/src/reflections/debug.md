# Debugging

Here are a couple debugging tricks I stumbled upon out as I wrote the vector.

## Pointers

One time, I had a problem where something was reading a null pointer. I thought
that after a swap, no thread could read the swapped in value, so I just swapped
in a null pointer. Sadly, I was mistaken. Looking at the error message, which
said `pointer 0x0 is invalid to dereference`, I got an idea. There were three
places swapping in a null pointer; the first I changed to swap in `0x1`, the
second `0x2`, and the third `0x3`.

After running the program through `Miri`, I got the error message
`pointer 0x2 is invalid to dereference`, and I knew where the bug was
originating from.

## `unwrap` vs. `expect`

Whenever you `unwrap` an `Option` or `Result` that is `None` or `Err`, Rust will
print out a little diagnostic saying where the `panic!` happened. I found it
helpful to use `expect` instead of `unwrap` because of the ability to provide
some extra context.

For example, there is a method in the `haphazard` crate called `AtomicPtr::load`
which returns an `Option<&T>`. It only returns a `None` value if the underlying
`AtomicPtr` contains a null pointer. Instead of `unwrap`ing the return value of
`load`, I called `expect("read null pointer")`. When I inevitably messed up and
`unwrap`ed a `None`, I new there was a null pointer floating around because of
the error message.

> Although these tricks seem small, they actually saved me a lot of time.
