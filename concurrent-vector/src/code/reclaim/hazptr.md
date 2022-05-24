# Hazard Pointers

The idea of hazard pointers is to _protect_ memory addresses from deallocation.
At any moment in time, we have a list of addresses that are not safe to reclaim.
We can store the addresses in a data structure like a concurrent linked lists; I
think this is what `haphazard`
[uses](https://docs.rs/haphazard/latest/src/haphazard/domain.rs.html#759-768).

Whenever we want to access a pointer, we access it through a _hazard pointer_.
When we access through a hazard pointer, the address we are accessing gets added
to the list of addresses to protect. When the hazard pointer get's dropped, or
we explicity dissasociate the hazard pointer from the underlying raw pointer,
the protection ends.

So why is this list important? When we are done with an object, we _retire_ it.
By retiring the pointer, we are agreeing to not use it anymore. Any thread that
is already accessing it can continue to do so, but there can be not _new_
readers/writers.

Every once in a while, the `Domain`, which holds the hazard pointers will go
through the `Retired` list. For each pointer, the `Domain` checks whether that
pointer is protected by reading the `Protected` list. If the pointer isn't
protected, the `Domain` deallocates it. If it is protected, the `Domain` does
not reclaim it, because someone is using it. In this way, we prevent pointers in
use from being deallocated, but those out of use are deallocated.

## An example

Hazard pointers are pretty complicated, so here's a visual example that I hope
helps.

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
tries to protect a pointer, if it's already protected, nothing will happen.

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

To use the hazard pointers, we're going to need to make a small change in the
vector's structure.
