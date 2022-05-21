# `new()`

We've got to have some way of making a vector (or at least for an outside user
to make one).

What are the ingredients we need to make the vector? Buffers, `Descriptor`, and
`WriteDescriptor`. The `WriteDescriptor` is going to be `None`, as we don't have
any pending writes yet.

Here's the code:

```rust
// We actually do want this to be copied
#[allow(clippy::declare_interior_mutable_const)]
const ATOMIC_NULLPTR: AtomicPtr<AtomicU64>
    = AtomicPtr::new(ptr::null_mut::<AtomicU64>());

pub fn new() -> Self {
    // Make an array of 60 AtomicPtr<Atomicu64> set to the null pointer
    let buffers = Box::new([ATOMIC_NULLPTR; 60]);
    // Make a new WriteDescriptor
    let pending = WriteDescriptor::<T>::new_none_as_ptr();
    // Make a new descriptor
    let descriptor = Descriptor::<T>::new_as_ptr(pending, 0, 0);
    // Return self!
    Self {
        descriptor: CachePadded::new(AtomicPtr::new(descriptor)),
        buffers: CachePadded::new(buffers),
        _boo: PhantomData,
    }
}

```

Firstly, we declare this constant, `ATOMIC_NULLPTR`. This is just an `AtomicPtr`
containging a null pointer. The reason the `const` declaration is necessary is
that when we make an array of something `[SOMETHING; 60]`, that `SOMETHING`
needs to be `Copy` or evaluatable at compile time. Since `AtomicPtr<AtomicU64>`
is not `Copy`, we resort to creating `ATOMIC_NULLPTR` at compile time. Once we
have our array of null pointers, we put it on the heap to reduce the size of the
vector. If we were carrying it all directly, the vector would be over 480 bytes
large! With a `Box`, we only store 8 bytes for the first level in our two-level
array.

Then, we make a `WriteDescriptor` using `new_none_as_ptr()`, which returns an
`Option<WriteDescriptor<T>>`. We pass this into the constructor (`new_as_ptr`) for
`Descriptor<T>`, and then assemble the `Descriptor` and the `Box`ed array
together to make the vector.

The constructors for the descriptor types end in `as_ptr` because they actually
return a raw pointer pointing to a heap allocation containing the value. We
achieve this by making a `Box` and then extracting the inner raw pointer.

```
let b = Box::(5);
let b_ptr = Box::into_raw(b); <- That's a raw pointer to heap memory!
```

## The Heap and the Stack

TODO: my first UB mistake
