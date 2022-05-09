# What are Memory Orderings?

In a concurrent environment, each variable has a modification history, all the
values it has been. Say we have a variable A. We could store 1 into it, then 2,
then 3.

The problem comes from the fact that another thread reading A can "read" any of
those values, **even after the last store is executed, "in real time".** For
example, it might have an older (stale) copy of the variable cached.

To ensure that our programs run the way we want, we need to specify more
explicitly which values in the modification history the CPU is allowed to use.

An _Ordering_ is a parameter you provide when
loading/storing/doing-anything-else with atomic variables that specifies these
constraints.

I'm not going to go super in-depth into the intricacies of each ordering, but I
will explain the important parts of each. If you're curious, Jon Gjenset has a
great youtube video on Atomics, which sections on each ordering:
[Crust of Rust: Atomics and Memory Ordering](https://www.youtube.com/watch?v=rMGWeSjctlY)

## Relaxed

The first ordering is `Relaxed`. This provides no guarantees on Ordering, simply
that loads/stores are concurrent. The classic use case (I think this use case is
classic at least, I always see it used in examples) of the `Relaxed` ordering is
incrememting/decrementing a counter. We don't really care about observing the of
the counter; we just want to make sure our updates happen. When we finally load
the counter, we can use an ordering with stronger guarantees.

## Release

This ordering is used with stores. You can think of `Release` as `Release`ing a
lock. We want any changes that happened while we had the lock to become visible
to other threads. When you store with `Release`, it's like saying "I'm done with
this, use these changes." More specifically, when you store with release, all
changes to the variable are ordered before any `Acquire` loads.

```
STORE (Relaxed) --
STORE (Release)  | // "Release the lock"
LOAD (Acquire)   |
    X          <-- // nope
```

The compiler can't reorder the `Relaxed` store after the `Release` store,
guaranteeing that other threads see both stores.

## Acquire

This is used with loads. You can think of `Acquire` like `Acquire`ing a lock.
This means that no operations should get reordered _before_ taking the lock.
When you load with `Acquire`, no reads or writes get reordered before that load.
Anything that happens after "taking the lock" stays after the "lock was taken"

```
    X                              <-| // nope
    X               <-|              | // nope
LOAD (Acquire)        |              | // "Take the lock"
STORE (Relaxed)      --              |
LOAD a different variable (Relaxed) --
```

Anything we do while "holding the lock", cannot get reorderd before "taking the
lock".

Note: Although the lock metaphor is helpful for understanding `Acquire` and
`Release`, remember there are no actual locks involved.

## AcqRel (Acquire _and_ Release)

An `AcqRel` load/store is just `Release` for stores and `Acquire` for loads. I
haven't really been able to discern when this ordering is used. I think one use
case is Read-Modify-Write operations, like loading a variable, multiplying it by
two, and storing it back. We want to the load to be `Acquire` and the store
`Release` so we would use `AcqRel` to achieve this. Even still, I have rarely
seen this Ordering used.

## SeqCst (Sequentially Consistent)

The `SeqCst` ordering makes has the same reordering effects of `AcqRel`, and also establishes a consistent modification order accross all threads. Two stores tagged `Relaxed` might show up in different orders to different threads. However, if they are both tagged `SeqCst`, they will show up in the same order to all threads. `SeqCst` is the strongest ordering, and thus also the safest (see Jon Gjenset's video for weird things that can happen). This comes at a price though, with the compiler often having to emit _memory fences_[^1] to guarantee sequential consistency. This can affect performance.

[^1] A memory fence prevents the CPU from reordering operations in certain ways. This is a great [article](https://preshing.com/20120710/memory-barriers-are-like-source-control-operations/) which describes many different types of fences, kind of like the different Atomic orderings, which restrict the compiler instead of the CPU.