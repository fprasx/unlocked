# Potential Optimizations

Each time we make a new `Descriptor` or `WriteDescriptor`, we allocate it on the
heap. This means we will make many heap allocations for only one `Descriptor` to
succeed at being `compare_exchange`'d in. What if we instead made one heap
allocation at the beginning of `push` and `pop`, and just overwrote the contents
on every iteration of the loop?

Well, I tried it, and the results range from worse for one microbenchmark to
being somewhat better on other microbenchmarks.

Here's the results of the vector we implemented:

```
test sealed::bench::pop                ... bench:     169,980 ns/iter (+/- 21,594)
test sealed::bench::push               ... bench:   1,025,550 ns/iter (+/- 43,945)
test sealed::bench::push_and_pop       ... bench:     829,768 ns/iter (+/- 63,895)
test sealed::bench::push_then_pop      ... bench:   1,732,666 ns/iter (+/- 113,670)
```

Here's the results for the modified vector:

```
test sealed::bench::pop                ... bench:     269,311 ns/iter (+/- 11,669)
test sealed::bench::push               ... bench:     962,469 ns/iter (+/- 23,620)
test sealed::bench::push_and_pop       ... bench:     786,135 ns/iter (+/- 32,104)
test sealed::bench::push_then_pop      ... bench:   1,611,816 ns/iter (+/- 68,167)
```

As you can see, `pop` (which is just a bunch of threads `pop`ing an empty
vector) is worse for the modified vector. The other microbenchmarks seem a
little better, but the intervals for the modified and original overlap, so I
doubt the change is significant.
