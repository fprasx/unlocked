# Memory Allocation

Memory allocation is probably the easiest part of implementing the vector. It's
almost the same as allocating memory for a normal vector, except the one small
part which involves actually mutating the vector. This part requires some
synchronization through compare-and-swap.
