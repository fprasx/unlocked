# Atomics

Besides having a cool name, atomics are crucial for writing concurrent code.

We first need to think about how computers perform operations. On a a 32-bit
machine, loading (reading) a 64-bit value would require two CPU operations, one
for the first 32 bits and one for the second 32 bits.

Suppose each box represents a byte (8 bits):

```

v    Load 1             v
+-----+-----+-----+-----+-----+-----+-----+-----+
|     |     |     |     |     |     |     |     |
+-----+-----+-----+-----+-----+-----+-----+-----+
                        ^         Load 2        ^
```

This shows how a load of a variable can take multiple steps.

**Atomic** operations take only one step. They have no intermediate observable
state, which means the CPU only observes them as having happened or not.

This is very important in multithreaded scenarios because if threads use
non-atomic operations, loads and scores might end up overlapping, resulting in
_torn_ reads and writes.

For example, on our hypothetical 32-bit machine, one core might finish the first
write to the 32-bit value, another core then might perform the two loads needed
to load the value, and then the first core might finish the storing the last 32
bits. Now, one core has a value that is half gibberish!

This is an example of a data race, an example of undefined behavior.
