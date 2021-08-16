#![feature(test)]

extern crate std;
extern crate test;

use header_vec::*;
use test::Bencher;

#[derive(Clone, Debug, PartialEq)]
#[repr(align(128))]
struct TestA {
    a: u32,
    b: usize,
    c: usize,
}

#[bench]
fn test_header_vec_create(b: &mut Bencher) {
    b.iter(|| {
        let mut v = HeaderVec::<TestA, usize>::new(TestA { a: 4, b: !0, c: 66 });
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
}

#[bench]
fn test_header_vec_with_zst_create(b: &mut Bencher) {
    b.iter(|| {
        let mut v = HeaderVec::<(), usize>::new(());
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
}

#[bench]
fn test_header_vec_with_u64_create(b: &mut Bencher) {
    b.iter(|| {
        let mut v = HeaderVec::<u64, usize>::new(2u64);
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
}

#[bench]
fn test_header_vec_with_three_word_create(b: &mut Bencher) {
    b.iter(|| {
        let mut v = HeaderVec::<(u64, u64, u64), usize>::new((2, 2, 2));
        const N_ELEMENTS: usize = 1000;
        for i in 0..N_ELEMENTS {
            v.push(i);
        }
        v
    });
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
