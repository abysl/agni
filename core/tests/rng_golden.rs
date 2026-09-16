use agni_core::Rng;

#[test]
fn the_raw_stream_is_pinned() {
    let mut rng = Rng::from_seed(0xC0FFEE);
    let stream: Vec<u64> = (0..8).map(|_| rng.next_u64()).collect();
    assert_eq!(
        stream,
        [
            0xfec5_7934_0d9c_0ad5,
            0x2f33_3d29_9677_249f,
            0x9fb6_1b68_efff_5620,
            0xf65f_a620_c865_aebd,
            0x8a0f_4987_4be1_f9e8,
            0x0ff2_6fac_0a74_245e,
            0xee7d_2f67_42a4_32ff,
            0x400a_32f0_a8a3_42c9,
        ]
    );
}

#[test]
fn the_bounded_stream_is_pinned() {
    let mut rng = Rng::from_seed(42);
    let draws: Vec<u32> = (0..16).map(|_| rng.below(52)).collect();
    assert_eq!(
        draws,
        [19, 19, 0, 51, 3, 18, 50, 23, 45, 31, 15, 2, 25, 1, 42, 44]
    );
}

#[test]
fn a_fisher_yates_shuffle_is_pinned() {
    let mut rng = Rng::from_seed(7);
    let mut deck: Vec<u32> = (0..10).collect();
    for i in (1..deck.len()).rev() {
        let j = rng.below((i + 1) as u32) as usize;
        deck.swap(i, j);
    }
    assert_eq!(deck, [6, 5, 4, 8, 9, 3, 0, 2, 1, 7]);
}

#[test]
fn the_zero_seed_substitute_is_pinned() {
    let mut rng = Rng::from_seed(0);
    assert_eq!(rng.next_u64(), 0x0d83_b3e2_9a21_487a);
    assert_eq!(rng.next_u64(), 0x54c4_4c79_f1fe_9d67);
}
