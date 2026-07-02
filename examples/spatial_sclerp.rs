//! Screw-linear interpolation (ScLERP) for rigid SE(3) transforms represented
//! as unit dual quaternions.
//!
//! Conventions here match `spatial_motor.rs`: Hamilton quaternions are stored as
//! `[w, x, y, z]`, a rigid transform is `M = r + eps d` with
//! `d = 0.5 * (0, t) * r`, composition is dual-quaternion multiplication, and
//! points transform as `R p + t`.
//!
//! For ScLERP we form the relative motor `m = M0^-1 * M1`, choose the shortest
//! unit dual-quaternion representative, decompose it as a screw
//!
//! `m = cos((theta + eps h) / 2) + (axis + eps moment) sin((theta + eps h) / 2)`,
//!
//! where `theta` is the rotation angle and `h` is translation along the screw
//! axis. The power `m^s` keeps the same screw line and scales only `theta` and
//! `h`; `sclerp(M0, M1, s) = M0 * m^s`. The zero-rotation case has no stable
//! screw axis, so it degenerates to straight-line translation.

use std::process::exit;

const EPS: f64 = 1.0e-12;
const CHECK_EPS: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug)]
struct Quat {
    w: f64,
    x: f64,
    y: f64,
    z: f64,
}

impl Quat {
    fn new(w: f64, x: f64, y: f64, z: f64) -> Self {
        Self { w, x, y, z }
    }

    fn identity() -> Self {
        Self::new(1.0, 0.0, 0.0, 0.0)
    }

    fn pure(v: [f64; 3]) -> Self {
        Self::new(0.0, v[0], v[1], v[2])
    }

    fn from_axis_angle(axis: [f64; 3], angle: f64) -> Self {
        let axis = vec_normed(axis);
        let half = 0.5 * angle;
        let (s, c) = half.sin_cos();
        Self::new(c, axis[0] * s, axis[1] * s, axis[2] * s).normalized()
    }

    fn conj(self) -> Self {
        Self::new(self.w, -self.x, -self.y, -self.z)
    }

    fn norm(self) -> f64 {
        (self.w * self.w + self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    fn normalized(self) -> Self {
        self / self.norm()
    }

    fn vector(self) -> [f64; 3] {
        [self.x, self.y, self.z]
    }

    fn rotate_vec(self, p: [f64; 3]) -> [f64; 3] {
        (self * Self::pure(p) * self.conj()).vector()
    }
}

impl std::ops::Add for Quat {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(
            self.w + rhs.w,
            self.x + rhs.x,
            self.y + rhs.y,
            self.z + rhs.z,
        )
    }
}

impl std::ops::Mul for Quat {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self::new(
            self.w * rhs.w - self.x * rhs.x - self.y * rhs.y - self.z * rhs.z,
            self.w * rhs.x + self.x * rhs.w + self.y * rhs.z - self.z * rhs.y,
            self.w * rhs.y - self.x * rhs.z + self.y * rhs.w + self.z * rhs.x,
            self.w * rhs.z + self.x * rhs.y - self.y * rhs.x + self.z * rhs.w,
        )
    }
}

impl std::ops::Mul<f64> for Quat {
    type Output = Self;

    fn mul(self, rhs: f64) -> Self::Output {
        Self::new(self.w * rhs, self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl std::ops::Div<f64> for Quat {
    type Output = Self;

    fn div(self, rhs: f64) -> Self::Output {
        Self::new(self.w / rhs, self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl std::ops::Neg for Quat {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.w, -self.x, -self.y, -self.z)
    }
}

#[derive(Clone, Copy, Debug)]
struct Motor {
    r: Quat,
    d: Quat,
}

impl Motor {
    fn identity() -> Self {
        Self {
            r: Quat::identity(),
            d: Quat::new(0.0, 0.0, 0.0, 0.0),
        }
    }

    fn from_translation(t: [f64; 3]) -> Self {
        Self::from_rotation_translation(Quat::identity(), t)
    }

    fn from_axis_angle_translation(axis: [f64; 3], angle: f64, t: [f64; 3]) -> Self {
        Self::from_rotation_translation(Quat::from_axis_angle(axis, angle), t)
    }

    fn from_rotation_translation(r: Quat, t: [f64; 3]) -> Self {
        let r = r.normalized();
        let d = Quat::pure(t) * r * 0.5;
        Self { r, d }.normalized()
    }

    fn normalized(self) -> Self {
        let n = self.r.norm();
        Self {
            r: self.r / n,
            d: self.d / n,
        }
    }

    fn compose(self, rhs: Self) -> Self {
        Self {
            r: self.r * rhs.r,
            d: self.r * rhs.d + self.d * rhs.r,
        }
        .normalized()
    }

    fn inverse(self) -> Self {
        Self {
            r: self.r.conj(),
            d: self.d.conj(),
        }
        .normalized()
    }

    fn transform_point(self, p: [f64; 3]) -> [f64; 3] {
        vec_add(self.r.rotate_vec(p), self.translation())
    }

    fn translation(self) -> [f64; 3] {
        (self.d * self.r.conj() * 2.0).vector()
    }

    fn shortest(self) -> Self {
        if self.r.w < 0.0 {
            Self {
                r: -self.r,
                d: -self.d,
            }
        } else {
            self
        }
    }
}

fn sclerp(m0: Motor, m1: Motor, s: f64) -> Motor {
    let relative = m0.inverse().compose(m1).shortest();
    m0.compose(screw_power(relative, s))
}

fn screw_power(m: Motor, s: f64) -> Motor {
    let m = m.normalized().shortest();
    let v = m.r.vector();
    let sin_half = vec_len(v);

    if sin_half.abs() < EPS {
        return Motor::from_translation(vec_scale(m.translation(), s));
    }

    let axis = vec_scale(v, 1.0 / sin_half);
    let theta = 2.0 * sin_half.atan2(m.r.w);
    let axial_translation = -2.0 * m.d.w / sin_half;
    let dual_vec = m.d.vector();
    let moment = vec_scale(
        vec_sub(
            dual_vec,
            vec_scale(axis, 0.5 * axial_translation * m.r.w),
        ),
        1.0 / sin_half,
    );

    let half = 0.5 * theta * s;
    let (scaled_sin, scaled_cos) = half.sin_cos();
    let scaled_axial = axial_translation * s;

    let r = Quat::new(
        scaled_cos,
        axis[0] * scaled_sin,
        axis[1] * scaled_sin,
        axis[2] * scaled_sin,
    );
    let dual_vec = vec_add(
        vec_scale(moment, scaled_sin),
        vec_scale(axis, 0.5 * scaled_axial * scaled_cos),
    );
    let d = Quat::new(
        -0.5 * scaled_axial * scaled_sin,
        dual_vec[0],
        dual_vec[1],
        dual_vec[2],
    );

    Motor { r, d }.normalized()
}

fn rotation_angle(q: Quat) -> f64 {
    let q = q.normalized();
    2.0 * vec_len(q.vector()).atan2(q.w.abs())
}

fn vec_add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn vec_sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn vec_scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn vec_len(a: [f64; 3]) -> f64 {
    vec_dot(a, a).sqrt()
}

fn vec_dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec_normed(a: [f64; 3]) -> [f64; 3] {
    vec_scale(a, 1.0 / vec_len(a))
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    vec_len(vec_sub(a, b))
}

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

fn close_vec(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
    distance(a, b) <= eps
}

fn report(ok: &mut bool, name: &str, passed: bool) {
    if passed {
        println!("[ok] {name}");
    } else {
        println!("[FAIL] {name}");
        *ok = false;
    }
}

fn main() {
    let mut ok = true;

    let m0 =
        Motor::from_axis_angle_translation([0.3, 0.4, 0.5], 0.7, [1.0, -2.0, 0.3]);
    let m1 = Motor::from_axis_angle_translation(
        [-0.2, 0.9, 0.1],
        -1.2,
        [-0.4, 0.8, 1.5],
    );
    let probe = [0.25, -0.5, 2.0];
    report(
        &mut ok,
        "ENDPOINTS s=0 matches M0",
        close_vec(
            sclerp(m0, m1, 0.0).transform_point(probe),
            m0.transform_point(probe),
            CHECK_EPS,
        ),
    );
    report(
        &mut ok,
        "ENDPOINTS s=1 matches M1",
        close_vec(
            sclerp(m0, m1, 1.0).transform_point(probe),
            m1.transform_point(probe),
            CHECK_EPS,
        ),
    );

    let axis = vec_normed([1.0, 2.0, -0.5]);
    let angle = 1.7;
    let rot_target = Motor::from_axis_angle_translation(axis, angle, [0.0, 0.0, 0.0]);
    for s in [0.25, 0.5, 0.75] {
        let got = rotation_angle(sclerp(Motor::identity(), rot_target, s).r);
        report(
            &mut ok,
            &format!("PURE ROTATION constant rate s={s}"),
            close(got, s * angle, CHECK_EPS),
        );
    }

    let t = [2.5, -1.0, 3.25];
    let trans_target = Motor::from_translation(t);
    for s in [0.25, 0.5, 0.75] {
        report(
            &mut ok,
            &format!("PURE TRANSLATION linear s={s}"),
            close_vec(
                sclerp(Motor::identity(), trans_target, s).translation(),
                vec_scale(t, s),
                CHECK_EPS,
            ),
        );
    }

    let mid = sclerp(m0, m1, 0.5);
    let a = [-1.0, 0.25, 0.75];
    let b = [0.5, 1.5, -0.25];
    report(
        &mut ok,
        "RIGIDITY preserves probe distance",
        close(
            distance(mid.transform_point(a), mid.transform_point(b)),
            distance(a, b),
            CHECK_EPS,
        ),
    );
    report(
        &mut ok,
        "RIGIDITY real quaternion norm is 1",
        close(mid.r.norm(), 1.0, CHECK_EPS),
    );

    let screw_target = Motor::from_axis_angle_translation(
        [0.0, 0.0, 1.0],
        std::f64::consts::PI,
        [0.0, 0.0, 4.0],
    );
    let screw_mid = sclerp(Motor::identity(), screw_target, 0.5);
    let screw_mid_angle = rotation_angle(screw_mid.r);
    let screw_mid_translation = screw_mid.translation();
    println!(
        "midpoint screw pose: angle={:.12}, translation=[{:.12}, {:.12}, {:.12}]",
        screw_mid_angle,
        screw_mid_translation[0],
        screw_mid_translation[1],
        screw_mid_translation[2],
    );
    report(
        &mut ok,
        "HELIX midpoint has 90 degree rotation",
        close(screw_mid_angle, 0.5 * std::f64::consts::PI, CHECK_EPS),
    );
    report(
        &mut ok,
        "HELIX midpoint has half axial translation",
        close_vec(screw_mid_translation, [0.0, 0.0, 2.0], CHECK_EPS),
    );

    if !ok {
        exit(1);
    }
}
