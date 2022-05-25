# Starting Code

## Pseudocode

This "pythonesque" pseudocode with some pointer operations thrown in shows the
general API and implementation details of the vector. The pseudocode is a
conversion of the paper's pseudocode into a more (in my opinion) understandable
form. It completely ignores memory reclamation.

You don't need to read this entire thing, it's just here as a reference.

```python
# Calculate the index of the correct bucket
# Return a pointer
def at(vector, i):
    pos = i + FIRST_BUCKET_SIZE
    hibit = highest_bit(pos)
    index = pos ^ 2 ** hibit
    return &vector.memory[hibit - highest_bit(FIRST_BUCKET_SIZE)][index]
```

```python
# Perform an atomic load at the correct index
def read(vector, i):
    return *at(vector, i).load(Ordering)
```

```python
# Perform an atomic store at the correct index
def write(vector, i, elem):
    return *at(vector, i).store(elem, Ordering)
```

```python
# Calculate the number of allocations needed
# Then perform each allocation
def reserve(vector, size):
    i = highest_bit(vector.descriptor.size + FIRST_BUCKET_SIZE - 1)
        - highest_bit(FIRST_BUCKET_SIZE)
    if i < 0 {
        i = 0
    }
    while i < highest_bit(size + FIRST_BUCKET_SIZE - 1)
        - highest_bit(FIRST_BUCKET_SIZE):
        i += 1
        allocate_bucket(vector, i)
```

```python
# Calculate the amount of memory needed
# Allocate that much memory
# Try to CAS it in
# If CAS fails, the bucket is already initalized, so free the memory
def allocate_bucket(vector, bucket):
    bucket_size = FIRST_BUCKET_SIZE * (2 ** bucket)
    mem = allocate(bucket_size)
    if not CAS(&vector.memory[bucket], nullptr, mem):
        free(mem)
```

```python
# Get the size of the current descriptor
# If there is a pending write operation, subtract one from the size
def size(vector):
    size = vector.descriptor.size
    if descriptor.writeop.pending:
        size -= 1
    return size
```

```python
# Get the current descriptor
# Complete a pending write operation
# Allocate memory if needed
# Make a new WriteDescriptor
# Try to CAS it in
# If CAS failed go back to first step
# Complete a pending write operation
def push(vector, elem):
    while True:
        current_desc = vector.descriptor
        complete_write(vector, current_desc.pending)
        bucket = highest_bit(current_desc.size + FIRST_BUCKET_SIZE)
            - highest_bit(FIRST_BUCKET_SIZE)
        if vector.memory[bucket] == nullptr:
            allocate_bucket(vector, bucket)
        writeop = WriteDescriptor(
            *at(vector, current_desc.size),
            elem,
            current_desc.size
        )
        next_desc = Descriptor(1 + current_desc.size, writeop)
        if CAS(&vector.descriptor, current_desc, next_desc):
            break
    complete_write(vector, next_desc.pending)
```

```python
# Get the current descriptor
# Complete a pending write operation
# Read the last element of the vector
# Make a new WriteDescriptor
# Try to CAS it in
# If CAS failed go back to first step
# Return the last element
def pop(vector):
    while True:
        current_desc = vector.descriptor
        complete_write(vector, current_desc.pending)
        elem = *at(current_desc.size - 1).load(Ordering)
        next_desc = Descriptor(curr_desc.size - 1, null)
        if CAS(&vector.descriptor, current_desc, next_desc):
            break
    return elem
```
