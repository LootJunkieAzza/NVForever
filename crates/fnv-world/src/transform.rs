//! Reference rotation convention.
//!
//! References store three Euler angles in radians (x, y, z). Matrices here are row-major
//! 3 x 3 rotating column vectors (`v' = M v`), the same convention as NIF transforms
//! (docs/spec-notes/nif.md §6). Which composition order and sign the engine uses is
//! decided by observation (world.md §4); [`Euler::ENGINE`] is that answer and
//! [`Euler::ALL`] lists the candidates the check tries.

pub type Mat3 = [f32; 9];

pub const IDENTITY: Mat3 = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

pub fn mul(a: &Mat3, b: &Mat3) -> Mat3 {
    let mut m = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            m[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum();
        }
    }
    m
}

pub fn transpose(a: &Mat3) -> Mat3 {
    [a[0], a[3], a[6], a[1], a[4], a[7], a[2], a[5], a[8]]
}

/// Counter-clockwise rotation by `a` radians about axis 0 (x), 1 (y) or 2 (z), looking
/// down the axis towards the origin (right-handed).
pub fn axis(i: usize, a: f32) -> Mat3 {
    let (s, c) = a.sin_cos();
    match i {
        0 => [1.0, 0.0, 0.0, 0.0, c, -s, 0.0, s, c],
        1 => [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c],
        _ => [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
    }
}

/// A composition order and sign for three stored angles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Euler {
    /// Axes in the order the matrices are multiplied left to right (`M = A0 A1 A2`), so
    /// the last one is applied to the vector first.
    pub order: [usize; 3],
    /// Angles are clockwise (negated) rather than counter-clockwise.
    pub clockwise: bool,
}

impl Euler {
    /// The convention the data uses: `M = Rx(-x) Ry(-y) Rz(-z)` [observed: static
    /// collections' combined meshes agree with their part placements under this
    /// convention for 286 of 308 parts with x/y rotation, the best alternative fits 85;
    /// the misfits concentrate in six collections. world.md §4,
    /// `examples/scol_rotation.rs`].
    pub const ENGINE: Euler = Euler { order: [0, 1, 2], clockwise: true };

    pub const ALL: [Euler; 12] = {
        const O: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let mut out = [Euler { order: [0, 1, 2], clockwise: false }; 12];
        let mut i = 0;
        while i < 12 {
            out[i] = Euler { order: O[i / 2], clockwise: i % 2 == 1 };
            i += 1;
        }
        out
    };

    pub fn matrix(&self, angles: [f32; 3]) -> Mat3 {
        let sign = if self.clockwise { -1.0 } else { 1.0 };
        let m = |i: usize| axis(self.order[i], sign * angles[self.order[i]]);
        mul(&mul(&m(0), &m(1)), &m(2))
    }

    pub fn name(&self) -> String {
        let axes = ['x', 'y', 'z'];
        let o: String = self.order.iter().map(|&i| axes[i]).collect();
        format!("{}{}", o, if self.clockwise { " cw" } else { " ccw" })
    }
}

/// Rotation of a reference with the engine's convention.
pub fn ref_rotation(angles: [f32; 3]) -> Mat3 {
    Euler::ENGINE.matrix(angles)
}

/// Largest absolute element difference.
pub fn max_diff(a: &Mat3, b: &Mat3) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_rotations_are_counter_clockwise() {
        // +90 degrees about z takes x to y.
        let m = axis(2, std::f32::consts::FRAC_PI_2);
        let v = [m[0], m[3], m[6]];
        assert!((v[0]).abs() < 1e-6 && (v[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn candidates_are_distinct_and_orthonormal() {
        let a = [0.3, -0.7, 1.1];
        let ms: Vec<Mat3> = Euler::ALL.iter().map(|e| e.matrix(a)).collect();
        for (i, m) in ms.iter().enumerate() {
            assert!(max_diff(&mul(m, &transpose(m)), &IDENTITY) < 1e-5);
            for n in &ms[..i] {
                assert!(max_diff(m, n) > 1e-3);
            }
        }
    }

    #[test]
    fn engine_is_transpose_of_zyx_counter_clockwise() {
        // Equivalent statement of the convention: rotate about z, then y, then x
        // counter-clockwise, applied to row vectors.
        let a = [0.4, -1.2, 2.9];
        let zyx = Euler { order: [2, 1, 0], clockwise: false }.matrix(a);
        assert!(max_diff(&ref_rotation(a), &transpose(&zyx)) < 1e-6);
    }

    #[test]
    fn single_axis_is_order_independent() {
        for e in Euler::ALL {
            let m = e.matrix([0.0, 0.0, 0.5]);
            let want = axis(2, if e.clockwise { -0.5 } else { 0.5 });
            assert!(max_diff(&m, &want) < 1e-6);
        }
    }
}
