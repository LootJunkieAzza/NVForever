use super::*;

#[test]
fn lod_stride_tiers_match_distance_and_facing() {
    let fwd = Vec3::Y;
    // Near in front -> every frame.
    assert_eq!(lod_stride(Vec3::new(0.0, 500.0, 0.0), fwd), 1);
    assert_eq!(lod_stride(Vec3::new(0.0, 1200.0, 0.0), fwd), 1);
    // Mid-range in front -> half rate.
    assert_eq!(lod_stride(Vec3::new(0.0, 1500.0, 0.0), fwd), 2);
    assert_eq!(lod_stride(Vec3::new(0.0, 2200.0, 0.0), fwd), 2);
    // Far in front -> quarter rate.
    assert_eq!(lod_stride(Vec3::new(0.0, 2500.0, 0.0), fwd), 4);
    // Close behind camera (<= 400 units) -> stays full rate.
    assert_eq!(lod_stride(Vec3::new(0.0, -350.0, 0.0), fwd), 1);
    // Behind camera beyond 400 units -> quarter rate.
    assert_eq!(lod_stride(Vec3::new(0.0, -500.0, 0.0), fwd), 4);
    // Sideways (dot == 0) at 600 units -> not behind, stays full rate.
    assert_eq!(lod_stride(Vec3::new(600.0, 0.0, 0.0), fwd), 1);
}

#[test]
fn lod_never_skips_initial_pose_or_combat_flinch_talk() {
    let cam = Some((Vec3::ZERO, Vec3::Y));
    let far = Some(Vec3::new(0.0, 3000.0, 0.0));
    // Initial sample is never skipped even when far away on a non-stride frame.
    assert!(!should_skip_lod_sample(false, false, false, false, cam, far, 1, 0));
    // Active flinch, talk clip, or combat attack is never skipped.
    assert!(!should_skip_lod_sample(true, true, false, false, cam, far, 1, 0));
    assert!(!should_skip_lod_sample(true, false, true, false, cam, far, 1, 0));
    assert!(!should_skip_lod_sample(true, false, false, true, cam, far, 1, 0));
    // Missing camera or actor transform never skips.
    assert!(!should_skip_lod_sample(true, false, false, false, None, far, 1, 0));
    assert!(!should_skip_lod_sample(true, false, false, false, cam, None, 1, 0));
}

#[test]
fn lod_staggered_frames_sample_exactly_once_per_stride() {
    let cam = Some((Vec3::ZERO, Vec3::Y));
    let mid = Some(Vec3::new(0.0, 1600.0, 0.0));
    let far = Some(Vec3::new(0.0, 3000.0, 0.0));
    // Over 4 consecutive frames, a mid-range actor samples 2 times and a far actor samples 1 time,
    // staggered across entity indices.
    for idx in 0..4u64 {
        let mid_sampled = (0..4u64)
            .filter(|&f| !should_skip_lod_sample(true, false, false, false, cam, mid, f, idx))
            .count();
        let far_sampled = (0..4u64)
            .filter(|&f| !should_skip_lod_sample(true, false, false, false, cam, far, f, idx))
            .count();
        assert_eq!(mid_sampled, 2);
        assert_eq!(far_sampled, 1);
    }
    // Two actors with adjacent indices do not both sample on the same frame at stride 4.
    let a0 = (0..4u64).position(|f| !should_skip_lod_sample(true, false, false, false, cam, far, f, 0)).unwrap();
    let a1 = (0..4u64).position(|f| !should_skip_lod_sample(true, false, false, false, cam, far, f, 1)).unwrap();
    assert_ne!(a0, a1);
}
