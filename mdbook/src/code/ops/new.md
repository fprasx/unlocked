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

    // Return self
    Self {
        descriptor: CachePadded::new(AtomicPtr::new(descriptor)),
        buffers: CachePadded::new(buffers),
        _boo: PhantomData,
    }
}

```

Firstly, we declare the constant `ATOMIC_NULLPTR`. This is just an `AtomicPtr`
containging a null pointer. The reason the `const` declaration is necessary is
that when we make an array of something `[SOMETHING; 60]`, that `SOMETHING`
needs to be `Copy` or evaluatable at compile time. Since `AtomicPtr<AtomicU64>`
is not `Copy`, we resort to creating `ATOMIC_NULLPTR` at compile time. Once we
have our array of null pointers, we put it on the heap to reduce the size of the
vector. If we were carrying it all directly, the vector would be over 480 bytes
large! With a `Box`, we only store 8 bytes pointing to the first level in our
two-level array.

Then, we make a `WriteDescriptor` using `new_none_as_ptr()`, which returns an
`Option<WriteDescriptor<T>>`. We pass this into the constructor (`new_as_ptr`)
for `Descriptor<T>`, and then assemble the `Descriptor` and the `Box`ed array
together to make the vector.

The constructors for the descriptor types end in `as_ptr` because they actually
return a raw pointer pointing to a heap allocation containing the value. We
achieve this by making a `Box` and then extracting the inner raw pointer.

```
let b = Box::(5);
let b_ptr = Box::into_raw(b); <- That's a raw pointer to heap memory!
```

## My first UB mistake

I introduced the heap and the stack earlier in the keywords section, but I
didn't explain why the distinction is important.

When a function is called, a _stack frame_ is pushed onto the stack. This stack
frame contains all the function's local variables. When the function returns,
the stack frame is popped off the stack, and all local variables are destroyed.
This invalidates all references to local variables that were just popped off.

The heap is different. You allocate on the heap, and you deallocate on the heap.
Nothing happens automatically. This is the legendary `malloc/free` combo from
`C`.

Understanding the distinction between the stack and the heap is important
because we are using raw pointers, which don't have the guarantees of
references.

Here is my first mistake, summarized a little:

```rust
use core::sync::atomic::{Ordering, AtomicPtr};

fn main() {
    let ptr = new_descriptor();
    // Use the pointer to the Descriptor
    let d = unsafe { &*ptr.load(Ordering::Acquire) };
}

// Return a pointer to a Descriptor
fn new_descriptor() -> AtomicPtr<Descriptor> {
    let d = Descriptor { size: 0, write: None };
    AtomicPtr::new(&d as *const _ as *mut _)
}

struct Descriptor {
    size: usize,
    write: Option<bool>
}

```

```
$ cargo miri run
```

```
error: Undefined Behavior: pointer to alloc1184 was dereferenced after this allocation got freed
  --> src\main.rs:46:22
   |
46 |     let d = unsafe { &*ptr.load(Ordering::Acquire) };
   |                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ pointer to alloc1184 was dereferenced after this allocation got freed
   |
   = help: this indicates a bug in the program: it performed an invalid operation, and caused Undefined Behavior
```

`Miri` says
`pointer to alloc1184 was dereferenced after this allocation got freed`.
Translation: `use-after-free`; classic UB.

So why is the `Descriptor`'s allocation being freed? Because it's **allocated on
the stack**. When `new_descriptor` returns, the local variable `d: Descriptor`
get's destroyed, and the pointer we made from the reference is invalidated.
Thus, we `use-after-free` when we deference a freed allocation.

This is the danger of using raw pointers. If we just passed on the reference
`Descriptor`, `Rust` would
[promote](https://rust-lang.github.io/rfcs/3027-infallible-promotion.html) that
value to have a `'static` lifetime if possible, or return an error if not. With
raw pointers, `Rust` doesn't manage lifetimes, so we have to ensure that our
pointers are valid.

This is why only dereferencing a raw pointer is `unsafe`. It's perfectly safe to
make one, but we have no guarantees about what it's pointing to, and that's why
the dereference is `unsafe`.

Thank you `Miri`!
