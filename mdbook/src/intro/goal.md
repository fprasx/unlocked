> You can read the code we'll write and the code for the book
> [here](https://github.com/fprasx/unlocked). The main files for the `Rust` code
> are [leaky.rs](https://github.com/fprasx/unlocked/blob/main/src/leaky.rs) and
> [sealed.rs](https://github.com/fprasx/unlocked/blob/main/src/sealed.rs).

# The Goal

This book has a few goals.

Inspired by
[Learn Rust With Entirely Too Many Linked Lists](https://rust-unofficial.github.io/too-many-lists/),
the main goal of this book is to teach you some Rust while implementing a useful
container. We'll be implementing the lock-free vector described in the paper
[Lock-free Dynamically Resizable Arrays](https://www.stroustrup.com/lock-free-vector.pdf)
by **Dechev et al., 2006**

I hope that this book will inspire other new Rustaceans like myself to push
their capabilities. I also hope that non-Rustaceans will see the how awesome
Rust is as well. No matter whether you code or not, I hope that this book will
show you a interesting area of computer science and a beautiful language!

## Topics We'll Cover

-   Concurrency
    -   Cache
    -   Exponential Backoff
-   Atomics
    -   Memory Orderings
    -   Compare-and-Swap
-   Memory Management
    -   Allocations in Rust
    -   Hazard Pointers
-   Using Rust
    -   `Box`
    -   `Drop`
-   Using `unsafe` Rust
    -   Raw Pointers
    -   How to write `unsafe` code
-   **Anything else I find interesting!**

## Necessary Experience

### tl;dr it's good to know some Rust

It will be helpful to be familiar with Rust or another language like C and C++,
as we will be dealing with low-level constructs like pointers, atomics, and
memory management. **However**, even if you are only familiar with `Some(_)` or
`None` of these things, I believe you will be able to learn an interesting thing
or two. I should say though, there is a _lot_ of code in the later portions of
the book.

Of course, the code will be in Rust, so prior knowledge will be helpful. I'm not
going to spend time explaining syntax. However, I will comment the code well and
explain what is going on. I think if you're comfortable with the first 15
chapters of [The Book](https://doc.rust-lang.org/book/), you should be fine.
Even if not, as long as you understand most of Rust syntax and are fine with
looking something up every once in a while, you'll be fine.
[Chapter 16](https://doc.rust-lang.org/book/ch16-00-concurrency.html) is very
helpful as well as it's the chapter on concurrency.
