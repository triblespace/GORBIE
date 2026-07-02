//! Minimal 3D projective geometric algebra motor kernel.
//!
//! This example uses Cl(3,0,1) with basis vectors `e0,e1,e2,e3`,
//! metric `e0^2 = 0`, `e1^2 = e2^2 = e3^2 = 1`. Blades are stored in
//! canonical ascending order by bitmask. Euclidean points are represented
//! by the dual homogeneous trivector
//! `P = e123 + x*e032 + y*e013 + z*e021`, so the normalized `e123`
//! coefficient is one. Motors are even multivectors with scalar,
//! Euclidean bivectors `e23,e31,e12`, ideal bivectors `e01,e02,e03`,
//! and pseudoscalar `e0123` coefficients. Translation by `t` is
//! `1 - 0.5*(t.x e01 + t.y e02 + t.z e03)`, rotation is
//! `cos(a/2) - sin(a/2)*B`, and every object below is transformed with
//! the same sandwich `M X ~M`.
//!
//! The point sandwich is validated against an inline dual-quaternion
//! oracle using `d = 0.5*(0,t)*r` and `transform_point = R p + t`.

use std::process;

const EPS: f64 = 1e-9;

const E0: usize = 1;
const E1: usize = 2;
const E2: usize = 4;
const E3: usize = 8;

const E01: usize = E0 | E1;
const E02: usize = E0 | E2;
const E03: usize = E0 | E3;
const E12: usize = E1 | E2;
const E13: usize = E1 | E3;
const E23: usize = E2 | E3;
const E0123: usize = E0 | E1 | E2 | E3;

const E012: usize = E0 | E1 | E2;
const E013: usize = E0 | E1 | E3;
const E023: usize = E0 | E2 | E3;
const E123: usize = E1 | E2 | E3;

#[derive(Clone, Copy, Debug)]
struct Mv {
    c: [f64; 16],
}

impl Mv {
    fn zero() -> Self {
        Self { c: [0.0; 16] }
    }

    fn scalar(s: f64) -> Self {
        let mut mv = Self::zero();
        mv.c[0] = s;
        mv
    }

    fn basis(mask: usize, value: f64) -> Self {
        let mut mv = Self::zero();
        mv.c[mask] = value;
        mv
    }

    fn gp(self, rhs: Self) -> Self {
        let mut out = Self::zero();
        for a in 0..16 {
            if self.c[a] == 0.0 {
                continue;
            }
            for b in 0..16 {
                if rhs.c[b] == 0.0 {
                    continue;
                }
                if let Some((mask, sign)) = blade_gp(a, b) {
                    out.c[mask] += self.c[a] * rhs.c[b] * sign;
                }
            }
        }
        out
    }

    fn wedge(self, rhs: Self) -> Self {
        let mut out = Self::zero();
        for a in 0..16 {
            if self.c[a] == 0.0 {
                continue;
            }
            for b in 0..16 {
                if rhs.c[b] == 0.0 || (a & b) != 0 {
                    continue;
                }
                let (mask, sign) = blade_wedge(a, b);
                out.c[mask] += self.c[a] * rhs.c[b] * sign;
            }
        }
        out
    }

    fn reverse(self) -> Self {
        let mut out = Self::zero();
        for mask in 0usize..16 {
            let grade = mask.count_ones();
            let sign = if (grade * grade.saturating_sub(1) / 2) % 2 == 0 {
                1.0
            } else {
                -1.0
            };
            out.c[mask] = self.c[mask] * sign;
        }
        out
    }

    fn dual(self) -> Self {
        let mut out = Self::zero();
        for mask in 0..16 {
            if self.c[mask] == 0.0 {
                continue;
            }
            let comp = (!mask) & E0123;
            let (_, sign) = blade_wedge(mask, comp);
            out.c[comp] += self.c[mask] * sign;
        }
        out
    }

    fn sandwich(self, object: Self) -> Self {
        self.gp(object).gp(self.reverse())
    }
}

fn blade_gp(a: usize, b: usize) -> Option<(usize, f64)> {
    let overlap = a & b;
    if (overlap & E0) != 0 {
        return None;
    }

    let mut sign = blade_sign(a, b);
    for bit in [E1, E2, E3] {
        if (overlap & bit) != 0 {
            sign *= 1.0;
        }
    }

    Some((a ^ b, sign))
}

fn blade_wedge(a: usize, b: usize) -> (usize, f64) {
    debug_assert_eq!(a & b, 0);
    (a | b, blade_sign(a, b))
}

fn blade_sign(a: usize, b: usize) -> f64 {
    let mut swaps = 0;
    for i in 0..4 {
        if (a & (1 << i)) == 0 {
            continue;
        }
        swaps += (b & ((1 << i) - 1)).count_ones();
    }
    if swaps % 2 == 0 {
        1.0
    } else {
        -1.0
    }
}

fn homogeneous_point_vector(p: [f64; 3]) -> Mv {
    let mut v = Mv::basis(E0, 1.0);
    v.c[E1] = p[0];
    v.c[E2] = p[1];
    v.c[E3] = p[2];
    v
}

fn point(p: [f64; 3]) -> Mv {
    homogeneous_point_vector(p).dual()
}

fn direction(v: [f64; 3]) -> Mv {
    let mut primal = Mv::zero();
    primal.c[E1] = v[0];
    primal.c[E2] = v[1];
    primal.c[E3] = v[2];
    primal.dual()
}

fn line_through(a: [f64; 3], b: [f64; 3]) -> Mv {
    homogeneous_point_vector(a)
        .wedge(homogeneous_point_vector(b))
        .dual()
}

fn point_to_xyz(p: Mv) -> [f64; 3] {
    let w = p.c[E123];
    [
        -p.c[E023] / w, // e032 = -e023
        p.c[E013] / w,
        -p.c[E012] / w, // e021 = -e012
    ]
}

fn direction_to_xyz(v: Mv) -> [f64; 3] {
    [-v.c[E023], v.c[E013], -v.c[E012]]
}

fn motor_from_translation(t: [f64; 3]) -> Mv {
    let mut m = Mv::scalar(1.0);
    m.c[E01] = -0.5 * t[0];
    m.c[E02] = -0.5 * t[1];
    m.c[E03] = -0.5 * t[2];
    m
}

fn motor_from_axis_angle(axis: [f64; 3], angle: f64) -> Mv {
    let axis = normalized(axis);
    let half = 0.5 * angle;
    let mut m = Mv::scalar(half.cos());
    let s = half.sin();
    m.c[E23] = -s * axis[0];
    m.c[E13] = s * axis[1]; // e31 = -e13
    m.c[E12] = -s * axis[2];
    m
}

fn motor_from_pose(axis: [f64; 3], angle: f64, t: [f64; 3]) -> Mv {
    compose(
        motor_from_translation(t),
        motor_from_axis_angle(axis, angle),
    )
}

fn compose(a: Mv, b: Mv) -> Mv {
    a.gp(b)
}

#[derive(Clone, Copy)]
struct DualQuat {
    r: [f64; 4],
    d: [f64; 4],
}

impl DualQuat {
    fn from_pose(axis: [f64; 3], angle: f64, t: [f64; 3]) -> Self {
        let axis = normalized(axis);
        let half = 0.5 * angle;
        let r = [
            half.cos(),
            axis[0] * half.sin(),
            axis[1] * half.sin(),
            axis[2] * half.sin(),
        ];
        let d = quat_scale(quat_mul([0.0, t[0], t[1], t[2]], r), 0.5);
        Self { r, d }
    }

    fn transform_point(self, p: [f64; 3]) -> [f64; 3] {
        let rp = quat_mul(quat_mul(self.r, [0.0, p[0], p[1], p[2]]), quat_conj(self.r));
        let tq = quat_scale(quat_mul(self.d, quat_conj(self.r)), 2.0);
        [rp[1] + tq[1], rp[2] + tq[2], rp[3] + tq[3]]
    }

    fn rotate_vector(self, v: [f64; 3]) -> [f64; 3] {
        let rq = quat_mul(quat_mul(self.r, [0.0, v[0], v[1], v[2]]), quat_conj(self.r));
        [rq[1], rq[2], rq[3]]
    }
}

fn quat_mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

fn quat_conj(q: [f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}

fn quat_scale(q: [f64; 4], s: f64) -> [f64; 4] {
    [q[0] * s, q[1] * s, q[2] * s, q[3] * s]
}

fn normalized(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn f64(&mut self, min: f64, max: f64) -> f64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let unit = ((self.state >> 11) as f64) * (1.0 / ((1u64 << 53) as f64));
        min + (max - min) * unit
    }

    fn vec3(&mut self, min: f64, max: f64) -> [f64; 3] {
        [self.f64(min, max), self.f64(min, max), self.f64(min, max)]
    }
}

fn close3(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    (a[0] - b[0]).abs() < eps && (a[1] - b[1]).abs() < eps && (a[2] - b[2]).abs() < eps
}

fn close_mv(a: Mv, b: Mv, eps: f64) -> bool {
    a.c.iter()
        .zip(b.c.iter())
        .all(|(a, b)| (*a - *b).abs() < eps)
}

fn check(name: &str, passed: bool) -> bool {
    if passed {
        println!("[ok] {name}");
    } else {
        println!("[FAIL] {name}");
    }
    passed
}

fn oracle_match_check() -> bool {
    let mut rng = Lcg::new(0x515f_9e37_d1ce_cafe);
    for _ in 0..64 {
        let mut axis = rng.vec3(-1.0, 1.0);
        if axis[0].abs() + axis[1].abs() + axis[2].abs() < 0.1 {
            axis[2] = 1.0;
        }
        let angle = rng.f64(-std::f64::consts::PI, std::f64::consts::PI);
        let t = rng.vec3(-5.0, 5.0);
        let p = rng.vec3(-10.0, 10.0);

        let motor = motor_from_pose(axis, angle, t);
        let got = point_to_xyz(motor.sandwich(point(p)));
        let want = DualQuat::from_pose(axis, angle, t).transform_point(p);
        if !close3(got, want, EPS) {
            eprintln!("oracle mismatch: got {got:?}, want {want:?}");
            return false;
        }
    }
    true
}

fn z_90_check() -> bool {
    let motor = motor_from_axis_angle([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
    close3(
        point_to_xyz(motor.sandwich(point([1.0, 0.0, 0.0]))),
        [0.0, 1.0, 0.0],
        EPS,
    )
}

fn uniformity_check() -> bool {
    let axis = normalized([0.3, -0.4, 0.8]);
    let angle = 1.1;
    let t = [2.0, -1.0, 0.5];
    let motor = motor_from_pose(axis, angle, t);
    let oracle = DualQuat::from_pose(axis, angle, t);

    let p = [1.5, -2.0, 0.25];
    let dir = [0.25, 0.8, -0.4];
    let a = [-1.0, 0.5, 0.75];
    let b = [2.0, -0.25, 1.5];

    let point_ok = close3(
        point_to_xyz(motor.sandwich(point(p))),
        oracle.transform_point(p),
        EPS,
    );
    let direction_ok = close3(
        direction_to_xyz(motor.sandwich(direction(dir))),
        oracle.rotate_vector(dir),
        EPS,
    );

    let line = line_through(a, b);
    let transformed_line = motor.sandwich(line);
    let line_from_transformed_points = line_through(
        point_to_xyz(motor.sandwich(point(a))),
        point_to_xyz(motor.sandwich(point(b))),
    );
    let line_ok = close_mv(transformed_line, line_from_transformed_points, EPS);

    if !point_ok {
        eprintln!("uniform point sandwich failed");
    }
    if !direction_ok {
        eprintln!("uniform direction sandwich failed");
    }
    if !line_ok {
        eprintln!("uniform line sandwich failed");
        eprintln!("sandwich line: {:?}", transformed_line.c);
        eprintln!("joined points:  {:?}", line_from_transformed_points.c);
    }

    point_ok && direction_ok && line_ok
}

fn compose_check() -> bool {
    let a = motor_from_pose([1.0, 0.2, -0.1], 0.7, [0.5, -1.5, 0.25]);
    let b = motor_from_pose([-0.4, 0.9, 0.1], -1.2, [-2.0, 0.25, 1.0]);
    let p = point([1.25, -0.75, 2.5]);
    let via_chain = a.sandwich(b.sandwich(p));
    let via_composed = compose(a, b).sandwich(p);
    close_mv(via_chain, via_composed, EPS)
}

fn main() {
    let mut ok = true;
    ok &= check(
        "ORACLE MATCH: PGA point sandwich equals dual-quat R p + t",
        oracle_match_check(),
    );
    ok &= check("90 deg about z sends (1,0,0) to (0,1,0)", z_90_check());
    ok &= check(
        "UNIFORMITY: same sandwich transforms point, direction, and line",
        uniformity_check(),
    );
    ok &= check(
        "compose(A,B) sandwich equals A applied after B",
        compose_check(),
    );

    if !ok {
        process::exit(1);
    }
}
