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
    let mut head_array = HeaderVec::new(TestA { a: 4, b: !0 });

    let quote = "the quick brown fox jumps over the lazy dog";

    for a in quote.chars() {
        head_array.push(a);
    }

    assert_eq!(TestA { a: 4, b: !0 }, *head_array);
    assert_eq!(quote, head_array[..].iter().copied().collect::<String>());
    head_array.retain(|&c| !"aeiou".contains(c));
    assert_eq!(
        "th qck brwn fx jmps vr th lzy dg",
        head_array[..].iter().copied().collect::<String>()
    );
}
