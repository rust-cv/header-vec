//! Tests for the `std` feature of the `header_vec` crate.
#![cfg(feature = "std")]

use header_vec::*;
use xmacro::xmacro;

#[test]
fn test_extend() {
    let mut hv = HeaderVec::new(());
    hv.extend([1, 2, 3]);
    assert_eq!(hv.as_slice(), [1, 2, 3]);
}

#[test]
fn test_extend_ref() {
    let mut hv = HeaderVec::<(), i32>::new(());
    hv.extend([&1, &2, &3]);
    assert_eq!(hv.as_slice(), [1, 2, 3]);
}

#[test]
fn test_drain() {
    let mut hv = HeaderVec::from_header_slice((), [1, 2, 3, 4, 5, 6]);

    let drain = hv.drain(1..4);
    assert_eq!(drain.as_slice(), [2, 3, 4]);
    drop(drain);
    assert_eq!(hv.as_slice(), [1, 5, 6]);
}

xmacro! {
    $(
        // tests with simple i32 lists
        name:                  init:              range: replace:     drained: result:
        nop_begin              [1, 2, 3, 4, 5, 6] (0..0) []           []       [1, 2, 3, 4, 5, 6]
        nop_middle             [1, 2, 3, 4, 5, 6] (3..3) []           []       [1, 2, 3, 4, 5, 6]
        nop_end                [1, 2, 3, 4, 5, 6] (6..6) []           []       [1, 2, 3, 4, 5, 6]
        insert_begin           [1, 2, 3, 4, 5, 6] (0..0) [-1, 0]      []       [-1, 0, 1, 2, 3, 4, 5, 6]
        insert_middle          [1, 2, 3, 4, 5, 6] (3..3) [33, 34]     []       [1, 2, 3, 33, 34, 4, 5, 6]
        insert_end             [1, 2, 3, 4, 5, 6] (6..6) [7, 8]       []       [1, 2, 3, 4, 5, 6, 7, 8]
        remove_begin           [1, 2, 3, 4, 5, 6] (0..2) []           [1, 2]   [3, 4, 5, 6]
        remove_middle          [1, 2, 3, 4, 5, 6] (3..5) []           [4, 5]   [1, 2, 3, 6]
        remove_end             [1, 2, 3, 4, 5, 6] (4..)  []           [5, 6]   [1, 2, 3, 4]
        replace_begin_shorter  [1, 2, 3, 4, 5, 6] (0..2) [11]         [1, 2]   [11,3, 4, 5, 6]
        replace_middle_shorter [1, 2, 3, 4, 5, 6] (3..5) [44]         [4, 5]   [1, 2, 3, 44, 6]
        replace_end_shorter    [1, 2, 3, 4, 5, 6] (4..)  [55]         [5, 6]   [1, 2, 3, 4, 55]
        replace_begin_same     [1, 2, 3, 4, 5, 6] (0..2) [11, 22]     [1, 2]   [11, 22, 3, 4, 5, 6]
        replace_middle_same    [1, 2, 3, 4, 5, 6] (3..5) [44, 55]     [4, 5]   [1, 2, 3, 44,55, 6]
        replace_end_same       [1, 2, 3, 4, 5, 6] (4..)  [55, 66]     [5, 6]   [1, 2, 3, 4, 55, 66]
        replace_begin_longer   [1, 2, 3, 4, 5, 6] (0..2) [11, 22, 33] [1, 2]   [11, 22, 33, 3, 4, 5, 6]
        replace_middle_longer  [1, 2, 3, 4, 5, 6] (3..5) [44, 55, 66] [4, 5]   [1, 2, 3, 44, 55, 66, 6]
        replace_end_longer     [1, 2, 3, 4, 5, 6] (4..)  [66, 77, 88] [5, 6]   [1, 2, 3, 4, 66, 77, 88]
        big_nop                [[1; 64]; 64]      (0..0) [[0; 64]; 0] [[0; 64]; 0] [[1; 64]; 64]
    )

    #[test]
    fn $+test_splice_$name() {
        let mut hv = HeaderVec::from_header_slice((), $init);
        let splice = hv.splice($range, $replace);

        assert_eq!(splice.drained_slice(), $drained);
        drop(splice);
        assert_eq!(hv.as_slice(), $result);
    }
}

// testing the header_vec!() macro
#[test]
fn test_empty_with_default_header() {
    let v: HeaderVec<(), i32> = header_vec![];
    assert!(v.is_empty());
}

#[test]
fn test_empty_with_custom_header() {
    let v: HeaderVec<&str, i32> = header_vec!("header"; []);
    assert_eq!(*v, "header");
    assert!(v.is_empty());
}

#[test]
fn test_values_with_custom_header() {
    let v = header_vec!("header"; [1, 2, 3]);
    assert_eq!(*v, "header");
    assert_eq!(v.as_slice(), &[1, 2, 3]);
    assert_eq!(v.len(), 3);

    let v = header_vec!("header"; [1, 2, 3,]);
    assert_eq!(*v, "header");
    assert_eq!(v.as_slice(), &[1, 2, 3]);
}

#[test]
fn test_repetition_with_custom_header() {
    let v = header_vec!("header"; [42; 5]);
    assert_eq!(*v, "header");
    assert_eq!(v.as_slice(), &[42, 42, 42, 42, 42]);
    assert_eq!(v.len(), 5);
}

#[test]
fn test_repetition_with_default_header() {
    let v = header_vec![42; 5];
    assert_eq!(v.as_slice(), &[42, 42, 42, 42, 42]);
    assert_eq!(v.len(), 5);
}

#[test]
fn test_values_with_default_header() {
    let v = header_vec![1, 2, 3];
    assert_eq!(v.as_slice(), &[1, 2, 3]);
    assert_eq!(v.len(), 3);

    let v = header_vec![1, 2, 3,];
    assert_eq!(v.as_slice(), &[1, 2, 3]);
}

#[test]
fn test_non_copy_types() {
    let v = header_vec!("header"; [String::from("hello"), String::from("world")]);
    assert_eq!(*v, "header");
    assert_eq!(
        v.as_slice(),
        &[String::from("hello"), String::from("world")]
    );

    let v = header_vec![String::from("repeated"); 2];
    assert_eq!(
        v.as_slice(),
        &[String::from("repeated"), String::from("repeated")]
    );
}

#[test]
fn test_zero_repetitions() {
    let v: HeaderVec<(), i32> = header_vec![42; 0];
    assert!(v.is_empty());

    let v: HeaderVec<&str, i32> = header_vec!("header"; [42; 0]);
    assert_eq!(*v, "header");
    assert!(v.is_empty());
}

#[test]
fn test_single_element() {
    let v = header_vec![42];
    assert_eq!(v.as_slice(), &[42]);

    let v = header_vec!("header"; [42]);
    assert_eq!(*v, "header");
    assert_eq!(v.as_slice(), &[42]);
}
