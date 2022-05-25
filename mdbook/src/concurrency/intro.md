# Concurrency

**Concurrent** (Merriam-Webster): operating or occurring at the same time

Concurrent programming is simply programming that involves more than one event
happening at a time, in the sense that we think of events in a program
happening. In a non-concurrent program, if we wanted to perform two
calculations, we would perform one, and _then_ the other. In a concurrent
approach, we might spawn two threads, and assign each of them a calculation to
perform. A big idea in concurrent programming is having multiple processes
running at the same time. You can think of it like your computer running Firefox
_and_ Spotify at the same time.[^1]

On a hardware level, one way to implement concurrency to have multiple CPU cores
(processors, the chips that do the math). Thus, we can add two numbers on one
core while dividing two numbers on another core.

[^1] Your computer might actually just be switching between the applications
really fast if you only have one CPU core, giving the illusion of multiple
processes happening at the same time. Even if you have many cores, it's possible
that the the applications could be running on the same core. It's all up to the
task scheduler.
