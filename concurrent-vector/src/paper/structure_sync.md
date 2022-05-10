# Synchronization

Synchronization, that is, coordinating concurrent operations on the vector, is
achieved through two little data structures: the `Descriptor` and the
`WriteDescriptor`. As you might expect, the `Descriptor` describes the vector
and the `WriteDescriptor` describes a write operation.

### The Descriptor

The descriptor holds two values: a pointer to a `WriteDescriptor`, and a value
indicating the size/length of the vector.

### The WriteDescriptor

The WriteDescriptor holds three values: the location of the write (a
pointer-like object), an old value, and a new value. You might be wondering why
a `WriteDescriptor` holds an old value. The answer: `compare_exchange`.

## How does this actually help with synchronization?

> The major challenges of providing lock-free vector implementation stem from
> the fact that key operations need to atomically modify two or more
> non-colocated words (Dechev et. al., 2006)

This translates to, "We need to change two things (without locking the vector)
down to ensure the vector is in the right state." For a `push` operation, say,
we would need to change the _length_ of the vector and write the new _data_. We
also might need to allocate more memory, which just means more changes we need
to synchronize.

The descriptor system gets around this by saying, "If you want to change the
descriptor, you need to complete a pending write." Why does this ensure the
correct semantics? Consider this example from the paper:

> The semantics of the `pop_back` and `push_back` operations are guaranteed by
> the `Descriptor` object. Consider the case when a `pop_back` is interrupted by
> any matching number of `push_back` and `pop_back` operations. In a naive
> implementation, the size of the vector would appear unchanged when the
> original `pop_back` resumes and the operation could produce an erroneous
> result.

Under the "naive implementation", in this scenario, the vector might look like
`[1, 2, 3]`. Someone calls `pop`, and the vector should return `3`. However, the
thread gets _preempted_ (the OS says another thread can run, and the current
thread is paused), and the running thread executes a bunch of `pop`s and
`push`es. The vector is now `[4, 5, 6]`. When the original pop finally runs, it
returns `6`.

Let's consider when the first `push` happens after the original `pop` under the
correct implementation. When the `push` happens, it swaps in a new `Descriptor`,
which says that the size is now one bigger and points to a new `WriteDescriptor`
representing a `push` operation. Because it swapped in a `Descriptor`, it has to
complete the operation specified in the current `WriteDescriptor`, and the
original pop returns `3`, as it should.


**Summary of the vector's structure**:

<!-- prettier-ignore-start -->
```yaml
# Data Organization
Vector: 
    [Pointer -> Memory],
    Pointer -> Descriptor

Descriptor: 
    Pointer -> Possible WriteDescriptor, 
    Size

WriteDescriptor: 
    Pointer -> Element location, 
    New Element, 
    Old Element
```
<!-- prettier-ignore-end -->
