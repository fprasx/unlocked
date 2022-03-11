use lfvec::highest_bit;
use lfvec::secvec::FIRST_BUCKET_SIZE;
fn main() {
    reserve(200_000, 300_000);
}

fn allocate_bucket(bucket: usize) {
    // The shift-left is equivalent to raising 2 to the power of bucket
    let bucket_size = FIRST_BUCKET_SIZE * (1 << bucket);
    println!("Inside `allocate_bucket`: bucket_size = {bucket_size}");
}

fn reserve(current: usize, new: usize) {
    // Number of allocations needed for current size
    let mut num_current_allocs =
        highest_bit(current + FIRST_BUCKET_SIZE - 1).saturating_sub(highest_bit(FIRST_BUCKET_SIZE));
    // Compare num_current_allocs to number of allocations needed for size `new`
    println!("{num_current_allocs}");
    while num_current_allocs
        < highest_bit(new + FIRST_BUCKET_SIZE - 1).saturating_sub(highest_bit(FIRST_BUCKET_SIZE))
    {
        num_current_allocs += 1;
        println!("Allocating bucket: {num_current_allocs}");
        allocate_bucket(num_current_allocs as usize);
    }
}

fn get_index(i: usize) -> usize {
    let pos = i + FIRST_BUCKET_SIZE;
    let hibit = highest_bit(pos);
    pos ^ (1 << hibit)
}

#[cfg(test)]
mod test {
    #[test]
    fn make_vector() {
        let vector = lfvec::secvec::SecVec::<usize>::new();
    }
}
