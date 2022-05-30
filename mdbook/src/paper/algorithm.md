# The Algorithm

As I’ve said before, I think of the vector as two connected systems: memory and
synchronization. By “The Algorithm”, I mean the synchronization aspect. To
recap, synchronization is controlled by two little data structures, the
`Descriptor` and the `WriteDescriptor`. These data structures describe the
vector itself and a write operation, respectively.

I think the best way to explain the algorithm is to dive right in.

## `complete_write()`

First, I want to explain a little routine called `complete_write`. This function
is true to its name and _completes_ a _write_.

> Write means "write operation", in this context, a `push` or `pop`. In my
> experience, "write" has been a more colloquial term used in CS for whenever we
> make a modification to something. Really anything can technically be a
> "write", but I would say things that are more final are "writes". For example,
> incrementing a loop variable is pretty insignificant in the grand scheme of
> things, so it's not really a "write", but increasing the size of the vector is
> an important "write". This usage might also be particular to concurrent
> programming, where balancing reads/writes is an important consideration for
> designing a data structure. Concurrent data structures are often designed for
> infrequent writes and frequent reads. Modifications to databases (which are
> **heavily** concurrent) can also be called writes. tl;dr a "write" in this
> case means the details describing a particular instance of "writing"

`complete_write` takes two arguments, a `WriteDescriptor`, and the vector
itself. `complete_write` applies the write operation described in the
`WriteDescriptor` on the vector. Recall that a `WriteDescriptor` contains three
things: a reference/pointer to the location where the write will take place, a
new value to write, and an old value that we loaded in from the location.

First we perform a `compare_exchange` using the data in the `WriteDescriptor`.
We only swap in the new data if the data at the location of the swap matches the
old data we have. If the `compare_exchange` succeeds, this means that we swapped
in the value we want to write. If it fails, it means someone else beat us to it
and performed the write. Remember, many threads can access the vector's
`Descriptor` and `WriteDescriptor` at once, so many threads will be trying to
complete the same write. Only one of them can succeed. It's a fight to the
death! Arrghhh!!!

I'm kidding. After performing the `compare_exchange`, successful for not, we
modify the vector to indicate that there is no pending write operation. If all
threads do this, at least once will succeed, and all will indicate that there is
no pending write operations. Though some of the threads may be sad because their
`compare_exchange` failed, the vector is happy because it's in a consistent and
correct state.

## `push()`

Now that we know writes are actually performed, let’s get into how a `push`
operation works. Here are the steps:

1. Load in the current `Descriptor`.
2. If the `Descriptor` contains a write operation, complete it . This is important
   because it ensures that before any new write operation happens, the previous
   one is completed. We cannot do anything before completing the previous write
   operation, so all operations _will_ eventually get executed.
3. Calculate which bucket our new element will go into.
4. If that bucket has not been allocated memory yet, do so.
5. Make a new `WriteDescriptor`. The `new` value in the `WriteDescriptor` will
   be the data passed into the `push` function.
6. Make a new `Descriptor` which contains the following data: the size held in
   the current `Descriptor` + 1, and the new `WriteDescriptor`.
7. Now, here comes the part that makes this a `compare-and-swap` or
   `compare_exchange` algorithm. We `compare_exchange` the new `Descriptor`
   we made with the old one. If the `Descriptor` held in the vector didn't
   change, our new `Descriptor` will replace it. If it did change, we will fail
   to swap in our new `Descriptor`, and we go back to Step 1.

   > Note: I think it's important to consider why this routine (particularly
   > step 6) ensures correctness. If the `compare_exchange` succeeds, this
   > means that the vector did not change in the time it took us to prepare a
   > new `Descriptor`. Why is this important? It means our assumptions about
   > the vector's state **did not change**. In our new `Descriptor`, we used
   > the size from the `Descriptor` we loaded in, and incremented that.
   > So, if the size we loaded in was `4`, our new `Descriptor` would say the
   > size of the vector is `5`. Now, imagine that we could just swap in our
   > fresh `Descriptor` without comparing it with the current one. If someone
   > else was also trying to `push`, their `Descriptor` might get swapped in
   > before ours. It would say the size of the vector is `5`, because it made
   > the same assumptions we did. Then we swap in our `Descriptor`, our
   > `Descriptor` would maintain that the size of the vector is `5`, even
   > though it should be `6` because there were two `push` operations.
   > Furthermore, we would overwrite the element that was `push`ed on by the
   > first call to `push`, because both our `WriteDescriptor`s would be
   > referencing the same location in memory. This is terrible!
   > `compare_exchange` is our friend.

8. Now that we have swapped in our `Descriptor`, we execute the
   `WriteDescriptor` we made using `complete_write`, finalizing the changes we
   want to make to the vector.

And that's a `push`!

`Pop` pretty much works the same except for some small variations, so we'll get
into that when we implement `push`/`pop`. However, the way we make sure changes
are valid using `compare_exchange` is identical for both operations.

I think it's finally time to start looking at some code. When I was writing
code, it felt very different from reasoning about the theory. I really felt like
I had to consider every line I wrote and every decision I made. I'll walk you
through what I came up with now.

> Note: we're going to first write a version of the vector that doesn't reclaim
> the memory it uses; it _leaks_.
