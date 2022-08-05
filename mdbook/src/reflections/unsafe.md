# `unsafe` code

As I said before, `unsafe` code isn't inherently bad, it's just code that comes
with a contract. Keeping this in mind helped me get over my initial apprehension
about using `unsafe` code.

If you write concurrent code, I think `unsafe` code is inevitable. There's just
too much to do with raw pointers and memory. Additionally, there are many more
contracts the compiler can't enforce in multithreaded scenarios.

Philosophically, concurrent code has to deal with shared mutable state,
otherwise it wouldn't do anything useful. Shared mutable state is inherently
unsafe! That's why `Rust` only allows one mutable reference (`&mut`) at a time:
it prevents memory bugs like data races. Thus, there is some danger
intrinsically associated with writing low-level concurrent code.

> "Shared mutable state is, among other things, the root of all evil" - Me, 2022

Although it seems scary at first, I'm really glad `Rust` has the concept of
`unsafe`. Whenever I had any memory bugs, I knew that the root cause must have
been in an `unsafe` block. Systematically checking over those blocks allowed me
to fix my code quickly.

It's good that we have to make explicit where we are doing potentially unsafe
things. Not just because of debugging, but because it makes us pause and check
everything over one more time. If nothing was `unsafe`, or everything was
`unsafe`, reasoning about our code would be much harder in my opinion.

> A note on debugging: **always** read the safety contract and document why what
> you're doing is safe! I caught so many bugs just by going over the safety
> contract again and realizing I wasn't following it.
