# Memory Reclamation

Allocation isn't the hard part when it comes to concurrency,
deallocation/reclamation is. When multiple threads/entities are concurrently
accessing an object, it is **never** safe to deallocate it without verifying
that no one has a reference/pointer to it. What if they were to use that pointer
after the deallocation?

This problem arises in the vector when reclaiming the `Descriptor`s and
`WriteDescriptors`. Multiple threads can hold a reference to them at once, so we
never know when it is safe to deallocation.

To solve this problem, we'll used technique called _hazard pointers_ via the
[`haphazard`](https://docs.rs/haphazard/latest/haphazard) crate.

> What is meant by reclamation/deallocation? When we allocate memory, the
> allocator returns a pointer to an allocation on the heap. Internally, the
> allocator also notes down that the space is being used. When we deallocate, or
> reclaim, memory, we return the pointer back to the allocator. The allocator
> goes back to the books and notes that no one is using the memory anymore,
> _freeing_ the memory. It can now hand that memory back out again if it's
> needed.

I'm not sure if this is the case, but I think the term "reclaim" might be used
specifically in concurrent contexts. Rust automatically deallocates memory in
single-threaded contexts using the borrow checker. We have to do everything
manually in multithreaded contexts
