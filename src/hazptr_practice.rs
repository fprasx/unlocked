// Practice example for using hazptrs to get a feel for API
extern crate alloc;
use alloc::boxed::Box;
use core::{marker::PhantomData, sync::atomic::AtomicPtr};

use haphazard::{self, raw::Pointer, Singleton};

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
    _marker: PhantomData<T>,
}

impl<T> DataPtr<T>
where
    T: Sized + Copy + Send + Sync,
{
    fn new(data: T) -> Self {
        Self {
            // this is safe because the ptr comes from into_raw
            data: unsafe { HazAtomicPtr::from(Box::new(data)) },
            domain: Domain::new(&Family {}),
            _marker: PhantomData::<T>,
        }
    }

    fn load(&self) -> T {
        let mut hp = HazardPointer::new_in_domain(&self.domain);
        // # Safety // Safe because the ptr is only accessed through hazptr mechanisms
        let data = unsafe { self.data.load(&mut hp) };
        *data.unwrap()
    }

    fn store(&self, data: T) {
        // just a simple cas loop
        // # SAFETY
        // this is safe because the ptr comes from into_raw
        let new = Box::new(data).into_raw();
        loop {
            // # SAFETY
            // this is safe because all hazptrs and loads are using the domain carried by the struct
            // The unwrap is safe because the pointer being derefed comes from Box::into_raw

            // # SAFETY
            // This is safe because `new` comes from Box::into_raw
            let s = unsafe {
                HazAtomicPtr::compare_exchange_weak_ptr(&self.data, self.data.load_ptr(), new)
            };
            match s {
                Ok(ptr) => {
                    if let Some(ptr) = ptr {
                        // # Safety
                        // Safe because the ptr is only accessed through hazptr mechanisms
                        unsafe { ptr.retire_in(&self.domain) };
                        break;
                    } else {
                        // None case, not sure when this would happen if cmpxchg is successful
                        continue;
                    }
                }
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    extern crate std;
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};
    use std::vec::Vec;

    // #[test]
    // fn leak() {
    // let data = Arc::new(DataPtr::new(1));
    // #[allow(clippy::needless_collect)]
    // let handles = (0..20)
    //     .map(|val| {
    //         let adata = Arc::clone(&data);
    //         if val % 2 == 0 {
    //             thread::spawn(move || {
    //                 for i in 0..100000 {
    //                     adata.store(2)
    //                 }
    //             })
    //         } else {
    //             thread::spawn(move || {
    //                 for _ in 0..100000 {
    //                     adata.load();
    //                 }
    //             })
    //         }
    //     })
    //     .collect::<Vec<JoinHandle<()>>>();
    // handles.into_iter().for_each(|h| h.join().unwrap());
    // }

    #[test]
    fn hazptr_send() {
        let hp: Arc<haphazard::AtomicPtr<isize>> =
            Arc::new(haphazard::AtomicPtr::from(Box::new(5)));
        thread::spawn(|| hp);
    }
}
