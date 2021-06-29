#[macro_use]
extern crate std;

use header_vec::*;

#[derive(Clone, Debug, PartialEq)]
struct TestA {
    a: u32,
    b: usize,
}

#[test]
fn test_head_array() {
    let mut v_orig = HeaderVec::new(TestA { a: 4, b: !0 });

    let quote = "the quick brown fox jumps over the lazy dog";

    for a in quote.chars() {
        v_orig.push(a);
    }

    assert_eq!(TestA { a: 4, b: !0 }, *v_orig);
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
}
