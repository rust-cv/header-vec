//! This file is partially is generated with github copilot assistance.
//! The only objective here is to generate coverage over all code paths to
//! make 'cargo mutants' pass. Then the test suite should pass
//! 'cargo +nightly miri test'. Unless otherwise noted the tests here are
//! not extensively reviewed for semantic correctness. Eventually human
//! reviewed tests here should be moved to other unit tests.

use header_vec::HeaderVec;

#[test]
fn test_drain_size_hint() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    let drain = vec.drain(1..4);
    let (lower, upper) = drain.size_hint();
    assert_eq!(lower, 3);
    assert_eq!(upper, Some(3));
}

#[test]
fn test_is_empty_exact() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    assert!(vec.is_empty_exact());

    vec.push(1);
    assert!(!vec.is_empty_exact());

    vec.truncate(0);
    assert!(vec.is_empty_exact());
}

#[test]
fn test_push_with_weakfix() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    let mut called = false;
    vec.push_with_weakfix(1, &mut |_| called = true);
    assert_eq!(vec.as_slice(), &[1]);

    // Test that push_with_weakfix actually pushes the value
    assert_eq!(vec.len(), 1);
    assert_eq!(vec[0], 1);

    // Test that multiple values can be pushed
    vec.push_with_weakfix(2, &mut |_| {});
    assert_eq!(vec.len(), 2);
    assert_eq!(vec[1], 2);
}

#[test]
fn test_splice_size_hint() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    let splice = vec.splice(1..4, [8, 9]);
    let (lower, upper) = splice.size_hint();
    assert_eq!(lower, 3);
    assert_eq!(upper, Some(3));
}

#[test]
fn test_reserve() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    let initial_cap = vec.capacity();
    vec.reserve(100);
    assert!(vec.capacity() >= initial_cap + 100);
}

#[test]
fn test_shrink_to() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    vec.reserve(100);
    let big_cap = vec.capacity();
    vec.shrink_to(10);
    assert!(vec.capacity() < big_cap);
    assert!(vec.capacity() >= 10);
}

#[test]
fn test_splice_drop() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    {
        let _splice = vec.splice(1..4, [8, 9]);
        // Let splice drop here
    }
    assert_eq!(vec.as_slice(), &[1, 8, 9, 5]);
}

#[test]
fn test_drain_debug() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let drain = vec.drain(1..);
    assert!(format!("{:?}", drain).contains("Drain"));
}

#[test]
fn test_is() {
    let vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let ptr = vec.ptr();
    assert!(vec.is(ptr));
    assert!(!vec.is(std::ptr::null()));
}

#[test]
fn test_shrink_to_fit_with_weakfix() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    vec.reserve(100);
    let big_cap = vec.capacity();
    let mut called = false;
    vec.shrink_to_fit_with_weakfix(&mut |_| called = true);
    assert!(vec.capacity() < big_cap);
    assert_eq!(vec.capacity(), vec.len());
}

#[test]
fn test_into_iter() {
    let vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let mut iter = (&vec).into_iter();
    assert_eq!(iter.next(), Some(&1));
    assert_eq!(iter.next(), Some(&2));
    assert_eq!(iter.next(), Some(&3));
    assert_eq!(iter.next(), None);
}

#[test]
fn test_len_strict() {
    let vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    assert_eq!(vec.len_strict(), 3);
    assert_eq!(vec.len_strict(), vec.len());
}

#[test]
fn test_drain_as_ref() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let drain = vec.drain(..);
    assert_eq!(drain.as_ref(), &[1, 2, 3]);
}

#[test]
fn test_drain_keep_rest() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4]);
    {
        let mut drain = vec.drain(..);
        assert_eq!(drain.next(), Some(1));
        drain.keep_rest();
    }
    assert_eq!(vec.as_slice(), &[2, 3, 4]);
}

#[test]
fn test_partial_eq() {
    let vec1: HeaderVec<&str, i32> = HeaderVec::from_header_slice("header1", [1, 2]);
    let vec2: HeaderVec<&str, i32> = HeaderVec::from_header_slice("header1", [1, 2]);
    let vec3: HeaderVec<&str, i32> = HeaderVec::from_header_slice("header2", [1, 2]);
    let vec4: HeaderVec<&str, i32> = HeaderVec::from_header_slice("header1", [1, 3]);

    assert_eq!(vec1, vec2);
    assert_ne!(vec1, vec3); // Different header
    assert_ne!(vec1, vec4); // Different elements
}

#[test]
fn test_splice_next_back() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4]);
    let mut splice = vec.splice(1..3, [5, 6]);
    assert_eq!(splice.next_back(), Some(3));
    assert_eq!(splice.next_back(), Some(2));
    assert_eq!(splice.next_back(), None);
}

#[test]
fn test_drain_keep_rest_tail_len() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    {
        let mut drain = vec.drain(1..3);
        drain.next();
        drain.keep_rest();
    }
    assert_eq!(vec.as_slice(), &[1, 3, 4, 5]);
}

#[test]
fn test_header_vec_basics() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());

    // Test empty state
    assert!(vec.is_empty());
    assert_eq!(vec.len(), 0);
    assert_eq!(vec.len_strict(), 0);
    assert_eq!(vec.as_slice().len(), 0);

    // Test non-empty state
    vec.push(1);
    assert!(!vec.is_empty());
    assert_eq!(vec.len(), 1);
    assert_eq!(vec.len_strict(), 1);
    assert_eq!(vec.as_mut_slice(), &mut [1]);

    // Test reserve_exact_with_weakfix
    let initial_cap = vec.capacity();
    let mut called = false;
    vec.reserve_exact_with_weakfix(10, &mut |_| called = true);
    assert!(vec.capacity() >= initial_cap + 10);

    // Test offset calculation implicitly through indexing
    vec.push(2);
    vec.push(3);
    assert_eq!(vec[0], 1);
    assert_eq!(vec[1], 2);
    assert_eq!(vec[2], 3);
}

#[test]
fn test_splice_drop_behavior() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    {
        let mut splice = vec.splice(1..3, vec![4, 5, 6]);
        assert_eq!(splice.next(), Some(2));
        // Let splice drop here with remaining elements
    }
    assert_eq!(vec.as_slice(), &[1, 4, 5, 6]);
}

#[test]
fn test_truncate_minus() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    vec.truncate(2);
    assert_eq!(vec.len(), 2);
    assert_eq!(vec.as_slice(), &[1, 2]);
}

#[test]
fn test_is_empty_strict() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    assert!(vec.is_empty_strict());
    vec.push(1);
    assert!(!vec.is_empty_strict());
}

#[test]
fn test_spare_capacity_mut() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::with_capacity(10, ());
    vec.push(1);
    let spare = vec.spare_capacity_mut();
    assert!(!spare.is_empty());
    assert_eq!(spare.len(), 9);
}

#[test]
fn test_weak_debug() {
    let vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let weak = unsafe { vec.weak() };
    assert!(!format!("{:?}", weak).is_empty());
}

#[test]
fn test_offset_alignment() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::with_capacity(1, ());
    vec.push(42);
    assert_eq!(vec[0], 42); // Tests correct memory layout/offset
}

#[test]
fn test_reserve_with_weakfix() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    let mut called = false;
    vec.reserve_with_weakfix(100, &mut |_| called = true);
    assert!(vec.capacity() >= 100);
}

#[test]
fn test_drop_behavior() {
    struct DropCheck(std::rc::Rc<std::cell::RefCell<bool>>);
    impl Drop for DropCheck {
        fn drop(&mut self) {
            *self.0.borrow_mut() = true;
        }
    }

    let dropped = std::rc::Rc::new(std::cell::RefCell::new(false));
    {
        let mut vec: HeaderVec<(), _> = HeaderVec::new(());
        vec.push(DropCheck(dropped.clone()));
    }
    assert!(*dropped.borrow());
}

#[test]
fn test_offset_arithmetic() {
    let mut vec: HeaderVec<(), u64> = HeaderVec::with_capacity(2, ());
    vec.push(123);
    vec.push(456);
    assert_eq!(vec[0], 123);
    assert_eq!(vec[1], 456);
}

#[test]
fn test_splice_debug() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let splice = vec.splice(1..3, vec![4, 5]);
    assert!(format!("{:?}", splice).contains("Splice"));
}

#[test]
fn test_extend_from_slice_with_weakfix() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::new(());
    let mut called = false;
    vec.extend_from_slice_with_weakfix([1, 2, 3], &mut |_| called = true);
    assert_eq!(vec.as_slice(), &[1, 2, 3]);
}

#[test]
fn test_truncate_plus() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    vec.truncate(2);
    assert_eq!(vec.len(), 2);
    assert_eq!(vec.as_slice(), &[1, 2]);

    // Test truncating to larger size (should have no effect)
    vec.truncate(10);
    assert_eq!(vec.len(), 2);
    assert_eq!(vec.as_slice(), &[1, 2]);
}

#[test]
fn test_into_iter_mut() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    let mut iter = (&mut vec).into_iter();

    // Verify we can mutate through the iterator
    if let Some(first) = iter.next() {
        *first = 100;
    }

    // Verify we get mutable references to all elements in correct order
    let mut collected: Vec<&mut i32> = iter.collect();
    *collected[0] = 200;
    *collected[1] = 300;

    // Verify mutations happened
    assert_eq!(vec.as_slice(), &[100, 200, 300]);
}

#[test]
fn test_len_exact() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3]);
    assert_eq!(vec.len_exact(), 3);

    vec.push(4);
    assert_eq!(vec.len_exact(), 4);

    vec.truncate(2);
    assert_eq!(vec.len_exact(), 2);

    // Additional checks that depend on len_exact
    assert_eq!(vec.capacity(), vec.len_exact() + vec.spare_capacity());
    assert_eq!(vec.len_exact(), vec.as_slice().len());
    assert_eq!(vec.is_empty_exact(), vec.len_exact() == 0);
}

#[test]
fn test_drain_keep_rest_advanced() {
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5]);
    {
        let mut drain = vec.drain(1..3);
        // Take one item to test non-zero start position
        assert_eq!(drain.next(), Some(2));
        drain.keep_rest();
    }
    // Verify that items are correctly placed when keeping rest with non-zero start position
    assert_eq!(vec.as_slice(), &[1, 3, 4, 5]);

    // Test with larger gaps to verify addition vs multiplication difference
    let mut vec: HeaderVec<(), i32> = HeaderVec::from([1, 2, 3, 4, 5, 6, 7, 8]);
    {
        let mut drain = vec.drain(2..6);
        assert_eq!(drain.next(), Some(3));
        drain.keep_rest();
    }
    assert_eq!(vec.as_slice(), &[1, 2, 4, 5, 6, 7, 8]);
}
