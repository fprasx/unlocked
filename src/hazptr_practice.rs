// Practice example for using hazptrs to get a feel for API
extern crate alloc;
use alloc::boxed::Box;
use core::{marker::PhantomData, sync::atomic::AtomicPtr};

use haphazard::{self, raw::Pointer};

// Setting up hazard pointers
// This makes sure they all use the same Domain, guaranteeing the protection is valid.
#[non_exhaustive]
struct Family;
type Domain = haphazard::Domain<Family>;
type HazardPointer<'domain> = haphazard::HazardPointer<'domain, Family>;
type HazAtomicPtr<T> = haphazard::AtomicPtr<T, Family>;

struct DataPtr<T> {
    data: HazAtomicPtr<T>,
    domain: Domain,
    _marker: PhantomData<T>
}

struct Data (usize, usize);

impl<T> DataPtr<T> where T: Copy {
    fn new(data: T) -> Self {
        Self {
            data: unsafe { HazAtomicPtr::new(Box::new(data).into_raw()) },
            domain: Domain::new(&Family {}),
            _marker: PhantomData::<T>

        }
    }

    fn load() -> T {
        todo!()
    }

    fn store(data: T) {
        // just a simple cas loop
        todo!()
    }
}


fn run_it() {
    todo!()
}