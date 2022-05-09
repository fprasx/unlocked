# Structure of the vector

This is where we begin actually working on the vector.

When thinking about the structure of the vector, I find it helpful to think
about it in two parts: memory and synchronization. By memory I mean allocation
of space and by synchronization I mean synchronizing reads/writes. Let's start
with memory.

## Memory

The vector is, broadly, a two-level array.

```
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 | Top-level
+---+---+---+---+---+
  |   |   |   |   |
  v   v   v   v   v
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 | Lower-level, notice these arrays are represented vertically
+---+---+---+---+---+
    | 2 | 3 | 4 | 5 |
    +---+---+---+---+
        | 3 | 4 | 5 |
        +---+---+---+
            | 4 | 5 |
            +---+---+
                | 5 |
                +---+

```

The vector stores a pointer to a first array (the top-level one). The elements
in this array are also pointers, to more arrays (the lower-level ones). This is
why this organization is called a two-level array.

The reason for this is resizing. Suppose we have 4 elements of capacity, and
they are all filled. We need to allocation more memory. We allocate more memory
using something like `malloc()`, and the allocator returns a pointer to the new
allocation.

For a normal vector, we would simply copy our vector's elements over to the new
allocation. We can't do this for a lockless vector though because copying isn't
atomic, and we can't lock down the vector, copy, and unlock. Therefore, we need
a different system.

> **A little tangent on allocations**: When allocating memory for a normal
> vector, we generally make larger and larger allocations. For example, the
> first allocation could be 4 elements, the next 8, then 16, 32 . . . This
> reduces the total number of allocations we need to perform, which is good for
> performance. We're going to use this same idea for the vector.

Returning to the idea of a two-level array, the first level is going to hold
pointers to blocks of memory we can call _buckets_. The amount of memory a
bucket holds is related to it's index. The first bucket will hold some constant
(which we'll call `FIRST_BUCKET_SIZE`) times 2 to the power of its index
elements. Here are some sample calculations for the first few buckets to show
the principle, using `FIRST_BUCKET_SIZE=8`:

```python
# Bucket 1
CAP = FIRST_BUCKET_SIZE * 2 ^ INDEX
    = 8 * 2 ^ 0
    = 8

# Bucket 2
CAP = FIRST_BUCKET_SIZE * 2 ^ INDEX
    = 8 * 2 ^ 1
    = 16
```

## Synchronization

Synchronization, that is, coordinating concurrent operations on the vector, is
acheived through two little data structures: the `Descriptor` and the
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

> The semantics of the `pop_back` and `push_back` opearations are guaranteed by
> the `Descriptor` object. Consider the case when a `pop_back` is interrupted by
> any matching number of `push_back` and `pop_back` operations. In a naive
> implementaion, the size of the vector would appear unchanged when the original
> `pop_back` resumes and the operation could produce an erroneous result.

Under the "naive implementaion", in this scenario, the vector might look like
`[1, 2, 3]`. Someone calls `pop`, and the vector should return `3`. However, the
thread gets _preempted_ (the OS says another thread can run, and the current
thread is paused), and the running thread executes a bunch of `pop`s and
`push`es. The vector is now `[4, 5, 6]`. When the original pop finally runs, it
returns `6`.

Let's consider when the first `push` happens after the original `pop` under the
correct implementation. When the `push` happens, it swaps in a new `Descriptor`,
which says that the size is now one bigger and points to a new `WriteDescriptor`
representing a `push` operation. Because it swapped in a `Descriptor`, it has to
complete the operation specified in the current `WriteDescriptor`, and the original
pop returns `3`, as it should.

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
