# Memory Allocation

The first thing I did when writing the code out was think about the pieces that
would make up the vector. In Rust, an extremely common building block for any
type is the `struct`. A `struct` just sticks its members' data next to each
other in memory. Here is the vector itself, as a struct:

```rust
pub struct SecVec<'a, T: Sized + Copy> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<AtomicPtr<Descriptor<'a, T>>>,
    _boo: PhantomData<T>, // Data is stored as transmuted T's
}
```

## Boo! 👻

I bet the `PhantomData` scared you. We have a generic parameter `T`, but we have
no `struct` members of `SecVec` or either of the descriptors that actually
contains a `T` (because we transmute `T` into `u64`s). Therefore, to let the
compiler know we really are carrying `T`'s, we add a little ghost that tells it,
"We're carrying this Phantom `T` _wink_ "

## Sharing is caring

There is a lot to unpack here. Firstly, `CachePadded` is a `struct` provided by
the `crossbeam_utils` crate.

> **A note on cache**: you may have heard of CPU cache, a small buffer of memory
> stored on the CPU to allow for fast access. The `cache` in `CachePadded`
> actually refers to a buffer between main RAM and the CPU's. It's just a larger
> and slower cache compared to a CPU cache. The cache is split into contiguous
> blocks of memory called _cache lines_. This is the most granular level at
> which cache coherency is maintained. When multiple threads both have a value
> in the same cache line, one thread modifying the value it owns marks the
> _entire_ cache line as "dirty". Even though the other thread's value hasn't
> been changed, the cache coherency protocol might cause the thread to reload
> the entire line when it uses the value, incurring some overhead. This is
> called _false sharing_, and cause severe performance degradation. Cache is an
> extremely important consideration when data structures. It's why linked lists
> are algorithmically fine but terribly slow in practice. As the saying goes,
> cache is king.

The `CachePadded` `struct` aligns its contents to the beginning of the cache
line to prevent false sharing. If all `CachePadded` objects are at the beginning
of a cache line (assuming they do not cross a cache line), there can't be false
sharing between them. Preventing false sharing can lead to a huge speedup, but
it also does increase the size of the type. If you're wondering how
`CachePadded` is implemented, check out
[`#[repr(align(n))]`](https://doc.rust-lang.org/nomicon/other-reprs.html) in the
Nomicon.

Here's how I picture cache padding:

```
|-----Cache line-----|-----Cache Line-----|
v                    v                    v
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
|69|xx|xx|xx|xx|xx|xx|42|xx|xx|xx|xx|xx|xx|
+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
^                    ^
 \                    \
  \                    \
   \                    \
    Different cache lines -> no false sharing
```

## Two-level array

The first member of `SecVec<T>` is a cache-padded array of 60 pointers allocated
on the heap (notice the `Box`). These pointers will each point into another
array. The pointers start off as null pointers (`0x0`), and will get swapped out
for valid pointers once they need to point to an actual array.

The `AtomicPtr`s point to `AtomicU64`s because each element is going to get
transmuted into a `u64` so that we can atomically perform writes on the vector.
When returning an element, we'll transmute it back into a T. _Transmuting_ means
interpreting the bits of one type as the bits of another.

For example, `0b10100001` means `-95` when interpreted as a signed integer but
`161` when interpreted as an unsigned integer. Transmuting one to the other
would just change how we interpret the bits, not tha actual bits themselves.

## Descriptors galore

The second member of `SecVec<T>` is a cache-padded `AtomicPtr` to a
`Descriptor`. As you've probably noticed, there are a bunch of `AtomicPtr`s
here. That's because we can modify the pointer atomically, specify which
`Ordering` to use, and `compare_exchange` the pointer. A common way of writing
data in concurrent programming is to change a pointer instead of actually
modifying a buffer. Since a buffer can't necessarily be modified atomically or
without locking, what we can do is prepare a buffer and then change a pointer so
that it points to our new buffer. All new readers will see the new data when
they dereference the pointer.

```
                 Pointer
                 /     \
                /       \
           +---+        +----+
          /                   \
         /         ->          \
        v                       v
       Old                      New
+---+---+---+---+        +---+---+---+---+
| 9 | 9 | 9 | 9 |        | 6 | 6 | 6 | 6 |
+---+---+---+---+        +---+---+---+---+
```

What do we do with the old pointer you might ask? Worry not, we will get into
that 😅

### The Descriptor and WriteDescriptor

```rust
pub struct Descriptor<'a, T: Sized> {
    pending: AtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
}

pub struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    _boo: PhantomData<T>, // New and old are transmuted T's
}
```

## The trait bounds

Notice how T is `Sized`, this means that its size is always known at
compile-time. We need to ensure this because our values need to be transmutable.
Part of the safety contract of `transmute_copy` is making sure our types are of
compatible sizes.

The `Copy` bound is necessary because the data in the vector is copied in and
out of the buffers, with `transmute_copy`.

OK, enough talk about `struct`s, let's get to the first function: `get()`
