# Structure of the vector

This is where we begin working on the vector.

When thinking about the structure of the vector, I find it helpful to think
about it in two parts: memory and synchronization. By memory I mean allocation
of space and by synchronization I mean synchronizing reads and writes. Let's
start with memory.

## Memory

The vector is, broadly, a two-level array.

```
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 | Top-level
+---+---+---+---+---+
  |   |   |   |   |
  v   v   v   v   v
+---+---+---+---+---+
| 1 | 2 | 3 | 4 | 5 | Lower-level, notice: these arrays are represented vertically
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
they are all filled. We need to allocate more memory. We allocate more memory
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
bucket holds is related to it's index. The first bucket will hold
`some constant (which we'll call FIRST_BUCKET_SIZE) times 2 to the power of its index`
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

3: 32
4: 64
To infinity and beyond . . .
```

The next part of the vector's structure is the synchronization aspect, which
goes hand in hand with explaining the algorithm. I'll cover them together.
