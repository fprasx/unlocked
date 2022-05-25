# Operations

Now that we have a solid memory backbone, we're going to implement the public
API of the vector: `new`, `push`, `pop`, and `size`, as well as `complete_write`
and some helper functions.

Since it's been a little bit, here's what the vector look like in code form again:

```rust
pub struct SecVec<'a, T: Sized + Copy> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<AtomicPtr<Descriptor<'a, T>>>,
    _boo: PhantomData<T>, // Data is stored as transmuted T's
}

struct Descriptor<'a, T: Sized> {
    pending: AtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
}

struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    _boo: PhantomData<T>, // New and old are transmuted T's
}

```
