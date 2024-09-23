#[macro_use]
extern crate std;

use header_vec::*;

#[derive(Clone, Debug, PartialEq)]
#[repr(align(128))]
struct TestA {
    a: u32,
    b: usize,
    c: usize,
}

#[test]
fn test_head_array() {
    let mut v_orig = HeaderVec::new(TestA { a: 4, b: !0, c: 66 });

    let quote = "the quick brown fox jumps over the lazy dog";

    for a in quote.chars() {
        v_orig.push(a);
    }

    assert_eq!(TestA { a: 4, b: !0, c: 66 }, *v_orig);
    assert_eq!(4, v_orig.a);
    assert_eq!(quote, v_orig[..].iter().copied().collect::<String>());

    let mut v_no_vowels = v_orig.clone();
    v_no_vowels.retain(|&c| !"aeiou".contains(c));
    assert_eq!(
        "th qck brwn fx jmps vr th lzy dg",
        v_no_vowels[..].iter().copied().collect::<String>()
    );

    v_orig.retain(|&c| !"aeiou".contains(c));

    assert_eq!(v_orig, v_no_vowels);
    assert_eq!(*unsafe { v_orig.weak() }, v_no_vowels);

    v_orig.retain(|&c| !"th".contains(c));

    assert_eq!(
        " qck brwn fx jmps vr  lzy dg",
        v_orig.as_slice().iter().copied().collect::<String>()
    );
}

// This shown a miri error
#[test]
fn test_push() {
    let mut hv = HeaderVec::with_capacity(10, ());

    hv.push(123);
    assert_eq!(hv[0], 123);
}

#[test]
fn test_extend_from_slice() {
    let mut hv = HeaderVec::new(());

    hv.extend_from_slice(&[0, 1, 2]);
    hv.extend_from_slice(&[3, 4, 5]);
    assert_eq!(hv.as_slice(), &[0, 1, 2, 3, 4, 5]);
}
