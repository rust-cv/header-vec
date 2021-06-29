#[macro_use]
extern crate std;

use header_vec::*;

#[derive(Debug, PartialEq)]
struct TestA {
    a: u32,
    b: usize,
}

#[test]
fn test_head_array() {
    let mut v = HeaderVec::new(TestA { a: 4, b: !0 });

    let quote = "the quick brown fox jumps over the lazy dog";

    for a in quote.chars() {
        v.push(a);
    }

    assert_eq!(TestA { a: 4, b: !0 }, *v);
    assert_eq!(4, v.a);
    assert_eq!(quote, v[..].iter().copied().collect::<String>());
    v.retain(|&c| !"aeiou".contains(c));
    assert_eq!(
        "th qck brwn fx jmps vr th lzy dg",
        v[..].iter().copied().collect::<String>()
    );
}
