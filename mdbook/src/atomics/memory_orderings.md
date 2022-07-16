# What are Memory Orderings?

In a concurrent environment, each variable has a modification history, all the
values it has been. Say we have a variable A. We could store 1 into it, then 2,
then 3.

The problem comes from the fact that another thread reading A can "read" any of
those values, **even after the last store is executed, "in real time".** For
example, it might have an older (stale) copy of the variable cached.

To ensure that our programs run the way we want, we need to specify more
explicitly which values in the modification history the CPU is allowed to use.

Another problem is the compiler reordering instructions. The Golden Rule of
instruction reordering is **do not modify the behavior of a single-threaded
program**. The compiler might not think it's doing anything wrong moving some
instructions around in one thread. And from the perspective of the thread that's
being modified, everything will seem alright. Other threads might start
receiving crazy results though.

An _ordering_ is a parameter you provide to operations with atomic variables
that specifies which reorderings can happen and which values in the modification
history the CPU can use.

I'm not going to go super in-depth into the intricacies of each ordering, but I
will explain the important parts of each. If you're curious, Jon Gjenset has a
great youtube video on Atomics, which sections on each ordering:
[Crust of Rust: Atomics and Memory Ordering](https://www.youtube.com/watch?v=rMGWeSjctlY).

> Going into the orderings, I find it helpful to separate their effects into two
> categories: those that have to do with compiler reordering, and those that
> have to do with the CPU. The compiler deals with the synchronization in the
> operation's thread, (well, actually the CPU does too, but that's a different
> story), and the CPU handles the synchronization across the other threads.

## Relaxed

The first ordering is `Relaxed`. When it comes to the CPU, there are no
guarantees imposed by this ordering. The compiler can reorder `Relaxed`
operations as long it follows the Golden Rule; it does not need to consider
other threads. The classic use case (I think this use case is classic at least,
I always see it used in examples) of the `Relaxed` ordering is
incrementing/decrementing a counter. We don't really care about observing the
state of the counter; we just want to make sure our updates happen correctly.
When we finally load the counter, we can use an ordering with stronger
guarantees.

## Release

`Release` is used with stores. You can think of `Release` as `Release`ing a
lock. We want any changes that happened while we had the lock to become visible
to other threads. When you store with `Release`, it's like saying "I'm done with
this, use these changes." Thus, the compiler cannot reorder operations _after_ a
`Release` store.

```
STORE (Relaxed) ─┐
STORE (Release) -+-// "Release the lock"
    X          <─┘ // nope, this happened while we "held the lock"
```

There is also a CPU property to this ordering, which I'll go over with
`Acquire`.

## Acquire

`Acquire` is used with loads. You can think of `Acquire` like `Acquire`ing a
lock. This means that no memory operations in the current thread can get
reordered _before_ taking the lock. Anything that happens after "taking the
lock" stays after the "lock was taken".

```
    X                              <─┐ // nope, this happened "after taking the lock"
    X               <─┐              │ // nope, this happened "after taking the lock"
LOAD (Acquire) -------+--------------+-// "Take the lock"
STORE (Relaxed)      ─┘              │
LOAD a different variable (Relaxed) ─┘
```

`Acquire` also has an important interaction with `Release` at the CPU level. Any
load `Acquire` or stronger must see the changes published by the release store
of the same variable.

> How does this achieve proper synchronization? You see, when two `Ordering`s
> love each other very much . . . we get `Acquire-Release` semantics. Watch what
> happens when we use `Acquire` and `Release` together (diagram inspired by
> [this blog post](https://preshing.com/20120913/acquire-and-release-semantics/)):
>
> <!-- prettier-ignore-start -->
>
> ```
> └───┘ Release store
>
>   | Read most recent data because the load is Acquire and the store is Release
>   V
>
> ┌───┐ Acquire load
> Memory operations cannot go above
>
>
> Memory operations cannot go below
> └───┘ Release store
>
>   | Read most recent data because the load is Acquire and the store is Release
>   V
>
> ┌───┐ Acquire load
> ```
>
> <!-- prettier-ignore-end -->
>
> All operations are trapped in their own sections, and each section gets the
> most recent modifications because of the way `Acquire` loads _synchronize_
> with the `Release` stores.

Note: Although the lock metaphor is helpful for understanding `Acquire` and
`Release`, remember there are no actual locks involved.

## AcqRel (Acquire _and_ Release)

An `AcqRel` load/store is just `Release` for stores and `Acquire` for loads.
When used with an operation that loads _and_ stores, it is both `Acquire` and
`Release`. `AcqRel`'s main use case is Read-Modify-Write operations, like
loading a variable, adding one, and storing it back. We want the load to be
`Acquire` and the store `Release` so we would use `AcqRel` to achieve this.
Foreshadowing: this ordering will play a prominent part later on!

## SeqCst (Sequentially Consistent)

The `SeqCst` ordering makes has the same reordering effects of `AcqRel`, and
also establishes a consistent modification order across all threads. Two stores
tagged `Relaxed` might show up in different orders to different threads.
However, if they are both tagged `SeqCst`, they will show up in the same order
to all threads. `SeqCst` is the strongest ordering, and thus also the safest
(see Jon Gjenset's video for weird things that can happen with weaker
orderings). Safety comes at a price though, with the CPU often having to
emit _memory fences_[^1] to guarantee sequential consistency. This can affect
performance.

[^1] A memory fence prevents the CPU from reordering operations in certain ways.
This is a great
[article](https://preshing.com/20120710/memory-barriers-are-like-source-control-operations/)
which describes many different types of fences, kind of like the different
Atomic orderings, which restrict the compiler instead of the CPU.
