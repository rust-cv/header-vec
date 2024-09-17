use core::mem::size_of_val;
use header_vec::HeaderVec;

#[derive(Debug)]
struct OurHeaderType {
    #[allow(dead_code)]
    a: usize,
}

fn main() {
    let h = OurHeaderType { a: 2 };
    let mut hv = HeaderVec::<OurHeaderType, char>::new(h);
    hv.push('x');
    hv.push('z');

    println!(
        "[`HeaderVec`] itself consists solely of a pointer, it's only {} bytes big.",
        size_of_val(&hv)
    );
    println!(
        "All of the data, like our header `{:?}`, the length of the vector: `{}`,",
        &*hv,
        hv.len()
    );
    println!(
        "and the contents of the vector `{:?}` resides on the other side of the pointer.",
        hv.as_slice()
    );
}
