# Building a Rusty, lock-free thread-safe vector

An implementation of the Lock-free vector described in the paper
[Lock-free Dynamically Resizable Arrays](https://www.stroustrup.com/lock-free-vector.pdf)
by **Dechev et al., 2006**

The implementation is not optimized for performance; it is solely academic.

## A book!

I wrote about the code itself and the experience writing it in an `mdbook`. If
you're interested in concurrency or seeing real `Rust`, you might like the book.
You can view it [here](https://fprasx.github.io/book/).

**Note**: I'm no longer maintaining the book in this repo, it's being maintained
in the [repo](https://github.com/fprasx/fprasx.github.io) for my website.

**Note**: Actually I need to figure out CI first so it's still being maintained
here.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
