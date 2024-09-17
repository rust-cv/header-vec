#![cfg(feature = "atomic_append")]
extern crate std;

use header_vec::*;

#[test]
fn test_atomic_append() {
    let mut hv = HeaderVec::with_capacity(10, ());

    hv.push(1);
    unsafe { hv.push_atomic(2).unwrap() };
    hv.push(3);

    assert_eq!(hv.len(), 3);
    assert_eq!(hv.as_slice(), [1, 2, 3]);
}
