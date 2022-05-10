# Compare-and-Swap (also know as CAS and Compare-Exchange)

Definition: swap a value with a new value _only if the the current value is what
we think it is._

This reason this is important is that loosely, we can say the state during the
swap is the same as the state we observed when preparing for.

Here's a code example:

```
LOAD A
CAS old = A, new = 2 * A // Only swap in the double if the number hasn't changed
```

A more realistic with a linked list would be this;

```
LOAD LIST_NODE

CAS old = LIST_NODE.pointer, new = new_pointer
```

In this case, we switch where the `LIST_NODE` points to only if it is pointing
to where it was when we first loaded it.

Here's what we did:

1. We loaded the node
2. We read the pointer
3. We CAS'd a new pointer

At this point, there are two things that can happen:

1. The pointer changed, and the CAS fails. This means that someone else changed
   the pointer first, and it's good that the CAS failed, because it's possible
   the the change that succeeded invalidates the change we are trying to make.
2. The pointer is the same, and CAS succeeds. Because the pointer was the same,
   our assumptions about the state of the vector held, and our change was valid.

At first, this might seem contrived and confusing (as it did to me). I would
focus on this intuition: _if CAS succeeds, loosely, we can say the state during
the swap was the same as the state we observed when preparing for the swap._ Our
assumptions were consistent throughout the whole process.

**Note**: for the rest of the book, I'm going to refer to compare-and-swap as `compare_exchange`, as that is what the Rust Standard Library uses.