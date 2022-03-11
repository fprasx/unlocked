# Pvec
Pvec<T\> is similar to a Vec<T\> but allows concurrent reading and writing. Safety is ensured by keeping track of which indices have writers and which have readers. Writing and reading follow Rust's normal borrowing rules.

The type is similar to a Vec<AtomicRefCell<T\>\>, except for atomic reference counting is built in. Size and capacity are also kept track of with atomics. 

Because this type relies heavily on atomics, it might be super slow. This is mostly just a proof of concept.

# Implementation

	* RawVec skeleton
	* AtomicUsize for capacity and len
	* Vec<AtomicUsize> for reference counting <- This might have to be a Pvec, in which we have a recursion problem 😱

# Resources
[atomic_refcell crate](https://crates.io/crates/atomic_refcell)

[Rust Stream: The Guard Pattern and Interior Mutability](https://www.youtube.com/watch?v=lmEKIvLh9D4)

[Crust of Rust: Smart Pointers and Interior Mutability](https://www.youtube.com/watch?v=8O0Nt9qY_vo)

[Implementing Rust's Vec From Scratch](https://www.youtube.com/watch?v=3OL95gZgPWA)
