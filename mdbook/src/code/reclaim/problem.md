# Memory Reclamation

Allocation isn't the hard part when it comes to concurrency, deallocation is.
When multiple threads/entities are concurrently accessing an object, it is
**never** safe to deallocate it without verifying that no one has a
reference/pointer to it. What is they were to use that pointer after the
deallocation?

This problem arises in the vector when deallocating the `Descriptor`s and
`WriteDescriptors`. Multiple threads can hold a reference to them at once, so we
never know when it is safe to deallocation.

To solve this problem, I used technique called _hazard pointers_ via the
[`haphazard`](https://docs.rs/haphazard/latest/haphazard) crate.
