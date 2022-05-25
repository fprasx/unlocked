# Hazard Pointers

The idea of hazard pointers is to _protect_ memory addresses from reclamation.
At any moment in time, we have a list of addresses that are not safe to reclaim.
We can store the addresses in a data structure like a concurrent linked list; I
think this is what `haphazard`
[uses](https://docs.rs/haphazard/latest/src/haphazard/domain.rs.html#759-768).

Whenever we want to access a pointer, we access it through a _hazard pointer_.
Accessing through a hazard pointer adds the address we are accessing to the list
of addresses to protect. When the hazard pointer gets dropped, or we explicitly
disassociate the hazard pointer from the underlying raw pointer, the protection
ends.

So why is the `Protected` list important? When we are done with an object, we
_retire_ it, marking it for eventual reclamation. By retiring the pointer, we
agree to not use it anymore. Any thread that is already accessing it can
continue to do so, but there can be no _new_ readers/writers.

Every once in a while, the `Domain`, which holds the hazard pointers, will go
through the `Retired` list. For each pointer on this list, the `Domain` checks
whether the pointer is protected by reading the `Protected` list. If the pointer
isn't protected, the `Domain` reclaims the object it points to (deallocating the
pointer). If it is protected, the `Domain` does not reclaim it, because someone
is using it. In this way, we prevent pointers in use from being deallocated, but
those out of use are deallocated.

## An example

Hazard pointers are pretty complicated, so here's a visual example that I hope
helps:

```
Protected: [1<0x22>]
Retired: []
              0x20   0x22   0x23   0x24
            +------+------+------+------+
Thread 1    |      |  <>  |      |      |
Thread 2    |      |      |      |      |
            +------+------+------+------+
```

Right now Thread 1 is accessing `0x22` via a hazard pointer, so the `Protected`
list contains the pointer `Ox22`, annotated with `1` to indicate Thread 1 is
protecting it. I'm not sure if you would actually keep track of which thread is
protecting a pointer in an actual implementation. I think if another thread
tries to protect an already protected pointer, nothing will happen.

Ok, now, Thread 2 accesses `0x22` and protects the pointer.

```
Protected: [1<0x22>, 2<0x22>]
Retired: []
              0x20   0x22   0x23   0x24
            +------+------+------+------+
Thread 1    |      |  <>  |      |      |
Thread 2    |      |  <>  |      |      |
            +------+------+------+------+
```

Thread 1 finishes with its access, and retires `0x22`. Thread 1 is saying, "No
one new will use this pointer, deallocate it when it's safe to do so!" `0x22` is
added to the `Retired` list. The `Domain` can't retire the pointer yet because
Thread 2 is still accessing it.

```
Protected: [2<0x22>]
Retired: [0x22]
              0x20   0x22   0x23   0x24
            +------+------+------+------+
Thread 1    |      |      |      |      |
Thread 2    |      |  <>  |      |      |
            +------+------+------+------+
```

Finally, Thread 2 finishes using the pointer, removing `0x22` from the
`Protected` list.

```
Protected: []
Retired: [0x22]
              0x20   0x22   0x23   0x24
            +------+------+------+------+
Thread 1    |      |      |      |      |
Thread 2    |      |      |      |      |
            +------+------+------+------+
```

The `Domain` sees that `0x22` is retired and no one is protecting it, so it
deallocates the allocation at `0x22`. We have reclaimed memory, and `0x22` will
not leak!

## Code changes

To use the hazard pointers, we're going to need to make a small change in the
vector's structure.

The hardest part was getting started.

Following the documentation on
[`Domain`](https://docs.rs/haphazard/latest/haphazard/struct.Domain.html), I
wrote a bunch of type `alias`es using the `type` keyword:

```rust
// Setting up hazard pointers
// This makes sure they all use the same Domain, guaranteeing the protection is valid.
#[non_exhaustive]
struct Family;
type Domain = haphazard::Domain<Family>;
type HazardPointer<'domain> = haphazard::HazardPointer<'domain, Family>;
type HazAtomicPtr<T> = haphazard::AtomicPtr<T, Family>;
```

We only use `Domain`s produced from struct `Family`. This prevents us from
retiring a pointer in the `Global` domain that is being guarded in a different
domain. The `Global` domain can't see the other `Domain`'s protected list, so
might prematurely retire the pointer.

Secondly, all the `HazardPointer`s and `HazAtomicPtr`s we construct will be in
same family as our `Domain`s. This ensures the same protection against
overlapping with the `Global` domain.

> The difference between `HazAtomicPtr` which is an an alias for
> `haphazard::AtomicPtr`, and `std::sync::atomic::AtomicPtr`, is that
> `HazAtomicPtr` uses hazard pointers to guard loads. Additionally, all atomic
> operations with `HazAtomicPtr` have `Acquire-Release` semantics built in.
> Nifty!

To ensure that we always retire and protect in the same domain, we will also
carry a `Domain` in the `struct` itself. Then, it's pretty easy to just always
use `&self.domain` whenever we need a `Domain`. All we have to do is add one more
`struct` field to `SecVec`:

```rust
pub struct SecVec<'a, T: Sized + Copy> {
    buffers: CachePadded<Box<[AtomicPtr<AtomicU64>; 60]>>,
    descriptor: CachePadded<HazAtomicPtr<Descriptor<'a, T>>>,
    domain: Domain, // Hi there :)
    _boo: PhantomData<T>,
}

struct Descriptor<'a, T: Sized> {
    pending: HazAtomicPtr<Option<WriteDescriptor<'a, T>>>,
    size: usize,
}

struct WriteDescriptor<'a, T: Sized> {
    new: u64,
    old: u64,
    location: &'a AtomicU64,
    _boo: PhantomData<T>,
}
```

And with that out of the way, we can now plug some leaks!
