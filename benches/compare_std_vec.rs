#![feature(test)]

extern crate std;
extern crate test;

use header_vec::*;
use test::Bencher;

#[derive(Clone, Debug, PartialEq)]
#[repr(align(128))]
struct TestA {
    a: usize,
    b: usize,
    c: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct TestAWithoutAlign {
    a: usize,
    b: usize,
    c: usize,
}

fn bench_with_header<H: Clone>(h: H, b: &mut Bencher) {
    b.iter(|| {
        let mut v = HeaderVec::<_, usize>::new(h.clone());
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
}

#[bench]
fn test_header_vec_with_zst_create(b: &mut Bencher) {
    bench_with_header((), b);
}

#[bench]
fn test_header_vec_with_one_word_create(b: &mut Bencher) {
    bench_with_header(2usize, b);
}

#[bench]
fn test_header_vec_with_three_word_create(b: &mut Bencher) {
    bench_with_header((2usize, 2usize, 2usize), b);
}

#[bench]
fn test_header_vec_with_test_a_create(b: &mut Bencher) {
    bench_with_header(TestA { a: 1, b: 1, c: 1 }, b);
}

#[bench]
fn test_header_vec_with_test_a_without_align_create(b: &mut Bencher) {
    bench_with_header(TestAWithoutAlign { a: 1, b: 1, c: 1 }, b);
}

#[bench]
fn test_regular_vec_create(b: &mut Bencher) {
    b.iter(|| {
        let mut v = Vec::<usize>::new();
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
}

// #[bench]
// fn test_header_vec_create_smaller(b: &mut Bencher) {
//     b.iter(|| {
//         let mut v = HeaderVec::<TestA, usize>::new(TestA { a: 4, b: !0, c: 66 });
//         const N_ELEMENTS: usize = 100;
//         for i in 0..N_ELEMENTS {
//             v.push(i);
//         }
//         v
//     });
// }

// #[bench]
// fn test_regular_vec_create_smaller(b: &mut Bencher) {
//     b.iter(|| {
//         let mut v = Vec::<usize>::new();
//         const N_ELEMENTS: usize = 100;
//         for i in 0..N_ELEMENTS {
//             v.push(i);
//         }
//         v
//     });
// }

// #[bench]
// fn test_header_vec_read(b: &mut Bencher) {
//     let mut v = HeaderVec::<TestA, usize>::new(TestA { a: 4, b: !0, c: 66 });
//     const N_ELEMENTS: usize = 1000;
//     for i in 0..N_ELEMENTS {
//         v.push(i);
//     }

//     b.iter(|| {
//         let mut acc = 0;
//         for i in 0..N_ELEMENTS {
//             acc += v[i];
//         }
//         acc
//     });
// }

// #[bench]
// fn test_regular_vec_read(b: &mut Bencher) {
//     let mut v = Vec::<usize>::new();
//     const N_ELEMENTS: usize = 1000;
//     for i in 0..N_ELEMENTS {
//         v.push(i);
//     }

//     b.iter(|| {
//         let mut acc = 0;
//         for i in 0..N_ELEMENTS {
//             acc += v[i];
//         }
//         acc
//     });
// }
