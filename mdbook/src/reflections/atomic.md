# Atomic Intuition

It's pretty safe to say that atomics are confusing. Just the context itself is
confusing: that CPUs and compilers can reorder instructions. I found that as I
developed an intuition for atomics, it became easier to reason about my code and
its correctness.

If you're getting started with concurrent programming, my advice would be to get
a solid grasp on atomics. You don't need to know every detail and all the ins
and outs. When you're writing code and you think "This `Acquire` load
synchronizes with that `Release` store", you gain confidence and it becomes
easier to get going.

The biggest moment for me was when I stopped having to look at the Standard
Library Documentation every time I used an atomic. I had developed an intuitive
sense of the orderings, and I could see why each one was useful in my code. At
first, I thought the orderings seemed a little random. As I started to
use atomics more and more, I saw how the orderings fit in nicely with actual use
cases, from using `Acquire` to load a bucket to `AcqRel` in `compare_exchange`.

Building an intuition for atomics is both satisfying and extremely useful.
