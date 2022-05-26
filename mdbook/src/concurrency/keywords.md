# Keywords

**Note**: don't read all of this if it's boring, just come back if there is a
word you don't know later on.

### Shared-state

A concurrency model where multiple threads operate on the same memory.

### Lock-free

A property of a system where after some finite number of time steps, a thread
will make progress.

> Ok, but what does lock-free actually mean? Suppose we have a thread holding a
> mutex. If that thread gets scheduled off by the OS, and never gets scheduled
> on again, no other threads can get the mutex and make progress. With a
> lock-free algorithm, we are guaranteed that after _some_ amount of time, at
> least one thread will make progress; one thread cannot block all of the others
> indefinitely.

### Buffer

Some block of memory that we use to hold data. The vector's buffers are where we
hold the actual elements.

### Cache

A type of memory that is faster to access but has less capacity that main memory
(RAM). If we need a value frequently, we can cache it so that accesses are
faster.

### Null Pointer

The pointer with address `0x0`. Never safe to dereference.

### Heap

A region of memory for storing long-lived or large values.

### Stack

A region of memory for storing local variables and small variables.

### Data race

When multiple threads access a value without synchronization and one of them is
writing. Picture a shared Google document. If people are trying to read it as
someone writes on it, they'll be reading gibberish until the writer is done.

### Thread

Like another program running within the main program. Threads can run the same
time as each other, and the first thread (the one we have at program start) is
called the _main thread_. Eventually, all threads are _joined_ back into the
main thread.

### Mutex {mut}ual {ex}clusion

A data structure that gives a thread exclusive access to some data. When you
call `lock` on a `Mutex`, you block until the `Mutex` is unlocked and you get
the lock. Then, you can do whatever you want with the data inside. Once you're
done, you unlock the `Mutex` to relinquish it back to the other threads.
