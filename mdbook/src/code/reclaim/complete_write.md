# Fixing complete_write

`complete_write` was the easiest leak to seal. When we swap out the
`WriteDescriptor`, we get back the old one. All we have to do is retire it, and
its memory will eventually get reclaimed.

We execute the `WriteDescriptor` and make a new one (`None`) like normal:

```rust
fn complete_write(&self, pending: *mut Option<WriteDescriptor<T>>) {
    // If cas of actual value fails, someone else did the write
    // Result of compare_exchange doesn't matter
    if let Some(writedesc) = unsafe { &*pending } {
        let _ = AtomicU64::compare_exchange(
            writedesc.location,
            writedesc.old,
            writedesc.new,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();

```

Here comes the part where the hazard pointers kick in. We make a hazard pointer
in `&self.domain`, then load in the `Descriptor`. Now, the current `Descriptor`
cannot get reclaimed as long as our hazard pointer is alive. Then we swap in a
new pointer to the `None` `WriteDescriptor`.

Here comes the big change, instead of just doing nothing with the pointer that
swapped out, we `retire` it in `&self.domain`. According to the documentation
for `retire_in`, there is a safety contract we need to follow (hence the marking
`unsafe fn`).

Let's look at that:

> 1. The pointed-to object will never again be returned by any
>    `[Haz]AtomicPtr::load`.
> 2. The pointed-to object has not already been retired.
> 3. All calls to load that can have seen the pointed-to object were using
>    hazard pointers from domain.

Alright, let's make sure we're fulfilling the contract.

Number one, we swapped out the pointer, so all new calls to `HazAtomicPtr::load`
will use the new pointer. This is `Acquire-Release` semantics in action under
the hood. Since the `swap_ptr` uses `Release`, all `HazAtomicPtr::load`s (which
use `Acquire`) will see the new value. Thus, the old value is safe from being
`load`ed again.

Number two, only one thread can get a pointer as the result of a swap. If I'm
holding a marble in my hand and I give it away, no one else can take that marble
from my hand. The person who took it can do whatever they want with it without
worrying about others interfering. Since we got the pointer as the result of
`swap_ptr`, no other thread has exclusive access like we do. We took the marble.
Therefore, we know that know other thread has already or might retire the
pointer. They can't access the marble anymore, and if we have the marble, it
means they never had it.

Finally, number 3, all operations (creating hazard pointers, retiring pointers)
happen through `&self.domain`!

After writing a 1000 word essay, we can confirm that `retire_in` is safe to
call. This is the argument we'll use for `retire`ing the results of
`compare_exchange` in `push`/`pop`.

```rust
        let mut hp = HazardPointer::new_in_domain(&self.domain);

        let old = unsafe {
            self.descriptor
                .load(&mut hp)
                .expect("ptr is null")
                .pending // This is a HazAtomicPtr<WriteDescriptor>
                // # Safety
                // new_writedesc conforms to the requirements of HazAtomicPtr::new()
                // because it comes from Box::into_raw and is a valid WriteDescriptor
                .swap_ptr(new_writedesc)
        };

        // # Safety
        // We are the only thread that will retire this pointer because
        // only one thread can get the result of the swap (this one).
        // Two threads couldn't have performed a swap and both got this pointer.
        unsafe { old.unwrap().retire_in(&self.domain) };

        // hp gets dropped, protection ends
    }
}

```

That's the only change to `complete_write`. `push`/`pop` aren't much worse.

---

### Complete source for `complete_write` (not leaky)

```rust
fn complete_write(&self, pending: *mut Option<WriteDescriptor<T>>) {
    // If cas of actual value fails, someone else did the write
    // Result of cmpxchng doesn matter
    if let Some(writedesc) = unsafe { &*pending } {
        let _ = AtomicU64::compare_exchange(
            writedesc.location,
            writedesc.old,
            writedesc.new,
            Ordering::AcqRel,
            Ordering::Relaxed,
        );

        let new_writedesc = WriteDescriptor::<T>::new_none_as_ptr();

        let mut hp = HazardPointer::new_in_domain(&self.domain);

        let old = unsafe {
            self.descriptor
                .load(&mut hp)
                .unwrap()
                .pending
                // # Safety
                // new_writedesc conforms to the requirements of HazAtomicPtr::new()
                // because it comes from Box::into_raw and is a valid WriteDescriptor
                .swap_ptr(new_writedesc)
        };

        // # Safety
        // We are the only thread that will retire this pointer because
        // only one thread can get the result of the swap (this one).
        // Two threads couldn't have performed a swap and both got this pointer.
        unsafe { old.unwrap().retire_in(&self.domain) };
    }
}

```
