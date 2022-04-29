use unlocked::hazptr_practice::*;
fn main() {
    let family = Family::new();
    let domain = Domain::new(&family);

    // Regular hazard pointer disposal
    let mut hp = HazardPointer::new_in_domain(&domain);
    let x = HazAtomicPtr::from(Box::new(5));
    // # Safety
    // The ptr will be retired through the domain
    let loaded = *unsafe { x.load(&mut hp) }.unwrap();
    println!("{loaded}");
    // Safe because x's hp was created in `domain`
    unsafe { x.retire_in(&domain) };

    // Dataptr disposal stuff
    let data = DataPtr::new(1);
    let load = data.load();
    println!("{load}");
    data.store(2);
    data.store(3);
}
