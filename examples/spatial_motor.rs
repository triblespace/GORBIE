//! M2 substrate — the `Motor` (SE(3)) encoding, verified headless.
//!
//! Realizes Decision 3/4 of the spatial-frames design: a rigid 3D
//! transform stored as 8×f32 (a unit dual quaternion — the even-grade
//! "motor" of 3D PGA, same 32 bytes either way) as `spatial::transform`.
//! Composition is the (dual-)quaternion product; a point transforms by
//! the rigid action `R p + t` decoded from it.
//!
//! The v0 *operations* here are the proven dual-quaternion formulas
//! (mathematically the motor; the PGA-blade sandwich and screw-motion
//! `sclerp` are the documented next layer). What is being validated is
//! the **encoding** (the core-grade contribution) and the **TF claim**:
//! that relating coordinate frames by a rigid transform actually works
//! for real geodata.
//!
//! The keystone test is the ground track: a point fixed in ECI, viewed
//! through the time-dependent ECI→ECEF Earth-rotation motor, must sweep
//! west in longitude by exactly the Earth-rotation angle. If the motor
//! reproduces that, "Earth's spin is a TF edge" is evidenced, not just
//! asserted.
//!
//! ```sh
//! cargo run --example spatial_motor --features triblespace
//! ```

use triblespace::core::inline::{Encodes, Inline, InlineEncoding, RawInline, TryFromInline};
use triblespace::core::metadata::{self, MetaDescribe};
use triblespace::core::trible::{Fragment, TribleSet};
use triblespace::macros::{find, id_hex, pattern};

// ── Quaternion (Hamilton, [w,x,y,z]) ─────────────────────────────────

type Quat = [f64; 4];
type Vec3 = [f64; 3];

fn q_mul(a: Quat, b: Quat) -> Quat {
    let [aw, ax, ay, az] = a;
    let [bw, bx, by, bz] = b;
    [
        aw * bw - ax * bx - ay * by - az * bz,
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
    ]
}
fn q_conj(a: Quat) -> Quat {
    [a[0], -a[1], -a[2], -a[3]]
}
fn q_axis_angle(axis: Vec3, angle: f64) -> Quat {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let (s, c) = (angle * 0.5).sin_cos();
    if n == 0.0 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    [c, axis[0] / n * s, axis[1] / n * s, axis[2] / n * s]
}
/// Rotate a vector by a unit quaternion: q (0,v) q*.
fn q_rotate(q: Quat, v: Vec3) -> Vec3 {
    let p = [0.0, v[0], v[1], v[2]];
    let r = q_mul(q_mul(q, p), q_conj(q));
    [r[1], r[2], r[3]]
}

// ── Motor = unit dual quaternion (real qr, dual qd) ──────────────────

/// A rigid SE(3) transform. `r` is the rotation quaternion; `d` is the
/// dual part `0.5 · (0,t) · r`, encoding the translation `t`.
#[derive(Clone, Copy, Debug)]
pub struct Motor {
    r: Quat,
    d: Quat,
}

impl Motor {
    fn identity() -> Self {
        Motor { r: [1.0, 0.0, 0.0, 0.0], d: [0.0; 4] }
    }
    /// Build from a rotation then a translation (applies R, then +t).
    fn from_rotation_translation(rot: Quat, t: Vec3) -> Self {
        let tq = [0.0, t[0], t[1], t[2]];
        let d = q_mul(tq, rot).map(|x| 0.5 * x);
        Motor { r: rot, d }
    }
    fn from_rotation(rot: Quat) -> Self {
        Motor::from_rotation_translation(rot, [0.0; 3])
    }
    fn from_translation(t: Vec3) -> Self {
        Motor::from_rotation_translation([1.0, 0.0, 0.0, 0.0], t)
    }
    /// Decode the translation: t = 2 · d · r*  (vector part).
    fn translation(&self) -> Vec3 {
        let t = q_mul(self.d.map(|x| 2.0 * x), q_conj(self.r));
        [t[1], t[2], t[3]]
    }
    /// Rigid action on a point: R p + t.
    fn transform_point(&self, p: Vec3) -> Vec3 {
        let rp = q_rotate(self.r, p);
        let t = self.translation();
        [rp[0] + t[0], rp[1] + t[1], rp[2] + t[2]]
    }
    /// Compose: `self ∘ other` applies `other` first, then `self`
    /// (matrix-multiplication order), via the dual-quaternion product.
    fn compose(&self, other: &Motor) -> Motor {
        Motor {
            r: q_mul(self.r, other.r),
            d: {
                let a = q_mul(self.r, other.d);
                let b = q_mul(self.d, other.r);
                [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]]
            },
        }
    }
}

// ── The Motor inline encoding (8×f32, 32 bytes exactly) ──────────────

/// 8 IEEE-754 f32 — `[r.w, r.x, r.y, r.z, d.w, d.x, d.y, d.z]` — filling
/// the value's 32 bytes exactly. f32 is ample for the sub-metre relative
/// transforms that make up almost every TF edge; root-anchoring absolute
/// positions stay in the f64 `Position` encoding instead.
pub struct MotorEnc;

impl MetaDescribe for MotorEnc {
    fn describe() -> Fragment {
        use triblespace::macros::entity;
        use triblespace::core::id::{ExclusiveId, Id};
        let id: Id = id_hex!("46C28F08205F0637CD28117B2A2B1B56");
        entity! {
            ExclusiveId::force_ref(&id) @
                metadata::name: "motor_f32x8",
                metadata::description: "Rigid SE(3) transform as a unit dual quaternion (PGA motor): 8 f32 [r.w,r.x,r.y,r.z, d.w,d.x,d.y,d.z], filling 32 bytes. Real part r is rotation; dual part d = 0.5*(0,t)*r encodes translation t. Composes by the dual-quaternion product; the same bytes admit the Cl(3,0,1) motor-sandwich interpretation.",
                metadata::tag: metadata::KIND_INLINE_ENCODING,
        }
    }
}
impl InlineEncoding for MotorEnc {
    type ValidationError = std::convert::Infallible;
    type Encoding = Self;
}
impl Encodes<Motor> for MotorEnc {
    type Output = Inline<MotorEnc>;
    fn encode(m: Motor) -> Inline<MotorEnc> {
        let mut raw: RawInline = [0u8; 32];
        let vals = [
            m.r[0] as f32, m.r[1] as f32, m.r[2] as f32, m.r[3] as f32,
            m.d[0] as f32, m.d[1] as f32, m.d[2] as f32, m.d[3] as f32,
        ];
        for (i, v) in vals.iter().enumerate() {
            raw[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        Inline::new(raw)
    }
}
impl TryFromInline<'_, MotorEnc> for Motor {
    type Error = std::convert::Infallible;
    fn try_from_inline(v: &Inline<MotorEnc>) -> Result<Self, Self::Error> {
        let mut f = [0.0f32; 8];
        let mut b = [0u8; 4];
        for (i, slot) in f.iter_mut().enumerate() {
            b.copy_from_slice(&v.raw[i * 4..i * 4 + 4]);
            *slot = f32::from_le_bytes(b);
        }
        Ok(Motor {
            r: [f[0] as f64, f[1] as f64, f[2] as f64, f[3] as f64],
            d: [f[4] as f64, f[5] as f64, f[6] as f64, f[7] as f64],
        })
    }
}

pub mod spatial {
    use super::MotorEnc;
    use triblespace::macros::attributes;
    attributes! {
        /// Rigid transform relating a child frame to its parent.
        "DDA86F42ECDC476BA60380734E916EF4" as transform: MotorEnc;
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn dist(a: Vec3, b: Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
fn lon_deg(p: Vec3) -> f64 {
    p[1].atan2(p[0]).to_degrees()
}

// ── Tests + the keystone ground-track demo ───────────────────────────

fn main() {
    use triblespace::core::id::fucid;
    use triblespace::macros::entity;
    use triblespace::core::inline::IntoInline;
    use std::f64::consts::PI;

    let mut fails = 0usize;
    let mut check = |name: &str, ok: bool| {
        println!("  [{}] {name}", if ok { "ok" } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    };

    println!("Motor (SE(3)) unit tests:\n");

    // 1. 90° about +z sends (1,0,0) → (0,1,0).
    let rz90 = Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], PI / 2.0));
    check("rotate +90° z: (1,0,0)→(0,1,0)", dist(rz90.transform_point([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]) < 1e-9);

    // 2. Pure translation.
    let tr = Motor::from_translation([3.0, -2.0, 5.0]);
    check("translate (1,1,1)→(4,-1,6)", dist(tr.transform_point([1.0, 1.0, 1.0]), [4.0, -1.0, 6.0]) < 1e-9);

    // 3. Rotation-then-translation order (R applied, then +t).
    let m = Motor::from_rotation_translation(q_axis_angle([0.0, 0.0, 1.0], PI / 2.0), [10.0, 0.0, 0.0]);
    check("R then +t: (1,0,0)→(10,1,0)", dist(m.transform_point([1.0, 0.0, 0.0]), [10.0, 1.0, 0.0]) < 1e-9);

    // 4. Composition == sequential application (the TF-chain property).
    let a = Motor::from_rotation_translation(q_axis_angle([1.0, 0.0, 0.0], 0.7), [1.0, 2.0, 3.0]);
    let b = Motor::from_rotation_translation(q_axis_angle([0.0, 1.0, 0.0], -1.1), [-4.0, 0.5, 2.0]);
    let p = [2.0, -1.0, 0.5];
    let composed = a.compose(&b).transform_point(p);
    let sequential = a.transform_point(b.transform_point(p));
    check("compose(a,b)·p == a·(b·p)  [TF chain]", dist(composed, sequential) < 1e-9);

    // 5. Identity is neutral under composition.
    check("identity ∘ a == a", dist(Motor::identity().compose(&a).transform_point(p), a.transform_point(p)) < 1e-12);

    // 6. Encode → store via entity! → query via pattern! → decode, then
    //    confirm the decoded f32 motor still transforms a point correctly
    //    (f32 precision, so a looser bound).
    let mut set = TribleSet::new();
    let id = fucid();
    set += entity! { &id @ spatial::transform: m.to_inline() };
    let rows: Vec<(Inline<MotorEnc>,)> = find!(
        (mot: Inline<MotorEnc>),
        pattern!(&set, [{ spatial::transform: ?mot }])
    )
    .into_iter()
    .collect();
    let decoded = Motor::try_from_inline(&rows[0].0).unwrap();
    check(
        "pile round-trip (8×f32): decoded motor transforms point to within 1e-4",
        rows.len() == 1 && dist(decoded.transform_point([1.0, 0.0, 0.0]), [10.0, 1.0, 0.0]) < 1e-4,
    );

    // ── Keystone: ground track via the ECI→ECEF Earth-rotation motor ──
    //
    // A point fixed in ECI (here a 7000 km-radius equatorial position at
    // ECI longitude 0). The ECI→ECEF transform at sidereal angle θ is a
    // rotation by −θ about z (ECEF has rotated +θ under the inertial
    // point). Its sub-satellite longitude must therefore read −θ exactly,
    // sweeping west as the Earth turns. This is the whole "Earth's spin
    // is a TF edge" claim, reduced to something checkable.
    println!("\nKeystone — ground track of a fixed-ECI point through the Earth-rotation motor:\n");
    let r_orbit = 7.0e6;
    let eci_point = [r_orbit, 0.0, 0.0]; // ECI longitude 0
    println!("    θ (deg)   sub-point lon (deg)   expected (−θ)   err");
    let mut track_ok = true;
    for step in 0..=8 {
        let theta = step as f64 / 8.0 * 2.0 * PI; // 0 .. 360°
        let eci_to_ecef = Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], -theta));
        let ecef = eci_to_ecef.transform_point(eci_point);
        let lon = lon_deg(ecef);
        let expected = (-theta.to_degrees() + 180.0).rem_euclid(360.0) - 180.0; // wrap to (−180,180]
        let err = {
            let mut e = (lon - expected).abs();
            if e > 180.0 {
                e = 360.0 - e;
            }
            e
        };
        println!("    {:7.1}   {:18.3}   {:13.1}   {:.2e}", theta.to_degrees(), lon, expected, err);
        if err > 1e-6 {
            track_ok = false;
        }
        // radius preserved (rigid motion) — a satellite doesn't change altitude under Earth spin.
        if (dist(ecef, [0.0; 3]) - r_orbit).abs() > 1e-3 {
            track_ok = false;
        }
    }
    println!();
    check("ground-track longitude == −θ at every step (rigid, radius-preserving)", track_ok);

    println!();
    if fails == 0 {
        println!("OK — all motor checks passed. The ECI→ECEF rotation motor reproduces a real");
        println!("ground track exactly, so a coordinate-frame relation realized as a rigid");
        println!("transform behaves correctly — first evidence the TF-via-motor design holds.");
    } else {
        println!("{fails} check(s) FAILED.");
        std::process::exit(1);
    }
}
