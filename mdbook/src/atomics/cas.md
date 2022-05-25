# Compare-and-Swap

_(also known as CAS and_ `compare_exchange`_)_

Definition: swap a value with a new value _only if the the current value is what
we think it is._

This reason this is important is that loosely, we can say the state during the
swap is the same as the state we observed when preparing for the swap.

Here's a code example:

```
LOAD A
CAS old = A, new = 2 * A // Only swap in the double if the number hasn't changed
```

A more realistic example with a linked list would be this;

```
LOAD LIST_NODE

CAS old = LIST_NODE.pointer, new = new_pointer
```

In this case, we switch the `LIST_NODE` pointer only if it hasn't changed.

Here's what we did:

1. We loaded the node
2. We read the pointer
3. We called CAS with the new pointer

At this point, there are two things that can happen:

1. The pointer changed, and the CAS fails. This means that someone else changed
   the pointer first, and it's good that the CAS failed, because it's possible
   the the change that succeeded invalidates the change we just tried to make.
2. The pointer is the same, and CAS succeeds. Because the pointer was the same,
   our assumptions about the state of the vector held, and our change was valid.

At first, this might seem contrived and confusing (as it did to me). I would
focus on this intuition: _if CAS succeeds, loosely, we can say the state during
the swap was the same as the state we observed when preparing for the swap._ Our
assumptions were consistent throughout the whole process.

The `compare_exchange` function in the Rust Standard Library returns a
`Result<T, T>`, where `T` is the type being exchanged. The `Result` contains the
value that the variable actually was. If `compare_exchange` fails, it returns
`Err(actual_value)`, on success, it returns `Ok(expected_value)` (if it
succeeded, that means `actual_value == expected_value`).

**Note**: for the rest of the book, I'm going to refer to `compare-and-swap` as
`compare_exchange`, as that is what the Rust Standard Library uses. I used
`compare-and-swap` on this page because the name is very explicit about what the
operation does.
