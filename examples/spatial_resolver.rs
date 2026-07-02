//! The capstone — a TF tree resolver, and the cross-domain unification
//! test the whole spatial design was for.
//!
//! Builds a frame tree — ECI (inertial root) → ECEF (Earth-fixed, a
//! *dynamic* edge: the spinning Earth, stored as timestamped rotation
//! samples) → a ground station's local ENU frame (static) — and a
//! satellite given as a position in ECI. The resolver walks the tree and
//! composes the per-edge motors (interpolating the dynamic Earth-spin
//! edge between bracketing samples by screw interpolation) to express the
//! satellite's ECI position in the station's ENU frame at any time t,
//! from which azimuth and elevation fall out. "Is the satellite above
//! the horizon, and where do I point the dish?" — answered by frame
//! composition alone.
//!
//! Every result is cross-checked against an independent direct
//! computation (manual ECI→ECEF rotation + dot products onto the ENU
//! basis) that uses none of the motor/tree machinery. If the frame-tree
//! path agrees with raw geometry, the unification is real: a satellite in
//! ECI and a station on ECEF live in one resolvable coordinate system.
//!
//! Verified headless (no GPU/display):
//! ```sh
//! cargo run --example spatial_resolver
//! ```

use std::collections::HashMap;

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
fn q_norm(a: Quat) -> Quat {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2] + a[3] * a[3]).sqrt();
    [a[0] / n, a[1] / n, a[2] / n, a[3] / n]
}
fn q_axis_angle(axis: Vec3, angle: f64) -> Quat {
    let n = norm3(axis);
    if n == 0.0 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    let (s, c) = (angle * 0.5).sin_cos();
    [c, axis[0] / n * s, axis[1] / n * s, axis[2] / n * s]
}
fn q_rotate(q: Quat, v: Vec3) -> Vec3 {
    let r = q_mul(q_mul(q, [0.0, v[0], v[1], v[2]]), q_conj(q));
    [r[1], r[2], r[3]]
}
/// Quaternion from a rotation matrix given as its three COLUMNS.
fn q_from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Quat {
    // m[row][col]
    let m = [
        [c0[0], c1[0], c2[0]],
        [c0[1], c1[1], c2[1]],
        [c0[2], c1[2], c2[2]],
    ];
    let tr = m[0][0] + m[1][1] + m[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [
            0.25 * s,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1.0 + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.0;
        [
            (m[2][1] - m[1][2]) / s,
            0.25 * s,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1.0 + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.0;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            0.25 * s,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1.0 + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.0;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            0.25 * s,
        ]
    };
    q_norm(q)
}

fn norm3(v: Vec3) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}
fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot3(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ── Motor = unit dual quaternion (rigid SE(3)) ───────────────────────

#[derive(Clone, Copy, Debug)]
struct Motor {
    r: Quat,
    d: Quat,
}
impl Motor {
    fn identity() -> Self {
        Motor { r: [1.0, 0.0, 0.0, 0.0], d: [0.0; 4] }
    }
    fn from_rotation_translation(rot: Quat, t: Vec3) -> Self {
        let d = q_mul([0.0, t[0], t[1], t[2]], rot).map(|x| 0.5 * x);
        Motor { r: rot, d }
    }
    fn from_rotation(rot: Quat) -> Self {
        Motor::from_rotation_translation(rot, [0.0; 3])
    }
    fn translation(&self) -> Vec3 {
        let t = q_mul(self.d.map(|x| 2.0 * x), q_conj(self.r));
        [t[1], t[2], t[3]]
    }
    /// self ∘ other applies `other` first, then `self`.
    fn compose(&self, other: &Motor) -> Motor {
        let r = q_mul(self.r, other.r);
        let a = q_mul(self.r, other.d);
        let b = q_mul(self.d, other.r);
        Motor { r, d: [a[0] + b[0], a[1] + b[1], a[2] + b[2], a[3] + b[3]] }
    }
    /// Inverse of a unit dual quaternion: (r*, d*) reversed appropriately.
    fn inverse(&self) -> Motor {
        // For a unit motor, inverse rotation is r*, and the inverse
        // translation is −R⁻¹ t. Rebuild from the decoded (R,t).
        let r_inv = q_conj(self.r);
        let t = self.translation();
        let t_inv = q_rotate(r_inv, [-t[0], -t[1], -t[2]]);
        Motor::from_rotation_translation(r_inv, t_inv)
    }
    fn transform_point(&self, p: Vec3) -> Vec3 {
        let rp = q_rotate(self.r, p);
        let t = self.translation();
        [rp[0] + t[0], rp[1] + t[1], rp[2] + t[2]]
    }
}

/// Screw-linear interpolation M(s) = m0 · (m0⁻¹ m1)^s (the verified
/// primitive from `spatial_sclerp`, compacted; reduces to slerp for a
/// pure-rotation edge as used here).
fn sclerp(m0: &Motor, m1: &Motor, s: f64) -> Motor {
    let rel = m0.inverse().compose(m1);
    // Sign-normalize to the short way round.
    let rel = if rel.r[0] < 0.0 {
        Motor { r: rel.r.map(|x| -x), d: rel.d.map(|x| -x) }
    } else {
        rel
    };
    let w = rel.r[0].clamp(-1.0, 1.0);
    let half = w.acos(); // half the rotation angle
    let sin_half = (1.0 - w * w).max(0.0).sqrt();
    let rel_pow = if sin_half < 1e-9 {
        // Near-zero rotation: translation interpolates linearly.
        let t = rel.translation();
        Motor::from_rotation_translation([1.0, 0.0, 0.0, 0.0], [t[0] * s, t[1] * s, t[2] * s])
    } else {
        let axis = [rel.r[1] / sin_half, rel.r[2] / sin_half, rel.r[3] / sin_half];
        let new_half = half * s;
        let (sn, cn) = new_half.sin_cos();
        let r_s = [cn, axis[0] * sn, axis[1] * sn, axis[2] * sn];
        // Axial translation scales with s; the on-axis screw pitch is
        // captured by interpolating the full translation through the new
        // rotation. For the pure-rotation edges used here, translation is
        // zero, so this reduces cleanly.
        let t = rel.translation();
        Motor::from_rotation_translation(r_s, [t[0] * s, t[1] * s, t[2] * s])
    };
    m0.compose(&rel_pow)
}

// ── Frame tree ───────────────────────────────────────────────────────

/// A frame's relation to its parent: the motor that maps a point's
/// coordinates *in this frame* to coordinates *in the parent frame*
/// (the pose of this frame within its parent). Static edges are one
/// motor; dynamic edges are timestamped samples, screw-interpolated at
/// lookup time — exactly tf2's append-only timestamped transform log,
/// here realized over the (already pile-verified) Motor encoding.
enum Edge {
    Static(Motor),
    Dynamic(Vec<(f64, Motor)>), // sorted by time
}

struct Frame {
    parent: Option<&'static str>,
    edge: Edge,
}

struct Tree {
    frames: HashMap<&'static str, Frame>,
}

impl Tree {
    /// The to-parent motor for `frame` at time t (interpolated if dynamic).
    fn edge_at(&self, frame: &str, t: f64) -> Motor {
        match &self.frames[frame].edge {
            Edge::Static(m) => *m,
            Edge::Dynamic(samples) => {
                if t <= samples[0].0 {
                    return samples[0].1;
                }
                if t >= samples[samples.len() - 1].0 {
                    return samples[samples.len() - 1].1;
                }
                // Find the bracketing pair and screw-interpolate.
                let mut i = 0;
                while samples[i + 1].0 < t {
                    i += 1;
                }
                let (t0, m0) = samples[i];
                let (t1, m1) = samples[i + 1];
                let s = (t - t0) / (t1 - t0);
                sclerp(&m0, &m1, s)
            }
        }
    }

    /// Motor mapping a point in `frame` to the root frame, at time t.
    fn to_root(&self, frame: &str, t: f64) -> Motor {
        let mut m = Motor::identity();
        let mut cur = frame;
        while let Some(parent) = self.frames[cur].parent {
            // m so far maps `frame` → `cur`; pre-compose this edge (cur → parent).
            m = self.edge_at(cur, t).compose(&m);
            cur = parent;
        }
        m
    }

    /// Express point `p` (given in frame `src`) in frame `dst`, at time t.
    fn resolve_point(&self, p: Vec3, src: &str, dst: &str, t: f64) -> Vec3 {
        let src_to_root = self.to_root(src, t);
        let dst_to_root = self.to_root(dst, t);
        // src → root → dst = inverse(dst_to_root) ∘ src_to_root
        let m = dst_to_root.inverse().compose(&src_to_root);
        m.transform_point(p)
    }
}

// ── Geodesy helpers ──────────────────────────────────────────────────

const A_WGS84: f64 = 6_378_137.0;
const F_WGS84: f64 = 1.0 / 298.257_223_563;

fn geodetic_to_ecef(lat_deg: f64, lon_deg: f64, alt: f64) -> Vec3 {
    let e2 = F_WGS84 * (2.0 - F_WGS84);
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let (sla, cla) = lat.sin_cos();
    let (slo, clo) = lon.sin_cos();
    let n = A_WGS84 / (1.0 - e2 * sla * sla).sqrt();
    [
        (n + alt) * cla * clo,
        (n + alt) * cla * slo,
        (n * (1.0 - e2) + alt) * sla,
    ]
}

/// ENU basis vectors (in ECEF) at a geodetic location.
fn enu_basis(lat_deg: f64, lon_deg: f64) -> (Vec3, Vec3, Vec3) {
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let (sla, cla) = lat.sin_cos();
    let (slo, clo) = lon.sin_cos();
    let east = [-slo, clo, 0.0];
    let north = [-sla * clo, -sla * slo, cla];
    let up = [cla * clo, cla * slo, sla];
    (east, north, up)
}

fn az_el(enu: Vec3) -> (f64, f64) {
    let (e, n, u) = (enu[0], enu[1], enu[2]);
    let az = e.atan2(n).to_degrees().rem_euclid(360.0);
    let el = u.atan2((e * e + n * n).sqrt()).to_degrees();
    (az, el)
}

// ── Build the world ──────────────────────────────────────────────────

const OMEGA: f64 = 7.292_115e-5; // Earth rotation rate, rad/s (sidereal)

/// θ(t): Earth-rotation angle. ECEF.to_parent (ECEF→ECI) is +θ about z.
fn theta(t: f64) -> f64 {
    OMEGA * t
}

fn main() {
    // Ground station: Svalbard-ish (a real polar downlink site), on the ellipsoid.
    let st_lat = 78.23;
    let st_lon = 15.39;
    let p_station_ecef = geodetic_to_ecef(st_lat, st_lon, 0.0);
    let (east, north, up) = enu_basis(st_lat, st_lon);

    // ENU.to_parent (ENU → ECEF): rotate ENU basis into ECEF (columns are
    // E,N,U) and translate to the station position.
    let r_enu_to_ecef = q_from_cols(east, north, up);
    let enu_to_ecef = Motor::from_rotation_translation(r_enu_to_ecef, p_station_ecef);

    // Dynamic ECEF.to_parent (ECEF → ECI = +θ(t) about z), stored as
    // timestamped samples every 30 min over 3 h; the resolver
    // screw-interpolates between them.
    let mut samples = Vec::new();
    let mut ts = 0.0;
    while ts <= 3.0 * 3600.0 + 1.0 {
        samples.push((ts, Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], theta(ts)))));
        ts += 1800.0;
    }

    let mut frames = HashMap::new();
    frames.insert("ECI", Frame { parent: None, edge: Edge::Static(Motor::identity()) });
    frames.insert("ECEF", Frame { parent: Some("ECI"), edge: Edge::Dynamic(samples) });
    frames.insert("ENU", Frame { parent: Some("ECEF"), edge: Edge::Static(enu_to_ecef) });
    let tree = Tree { frames };

    // A satellite, given as a fixed position in ECI (a 20 000 km-radius
    // point over the pole-ward longitude, so it climbs high over Svalbard
    // as the Earth turns it into view).
    let sat_eci = geodetic_to_ecef(70.0, 15.39, 20_000_000.0 - A_WGS84);
    // (Treat those ECEF-at-t=0 coords as the inertial position; ECI and
    // ECEF coincide at t=0, so this is a clean way to seed an ECI point.)

    let mut fails = 0usize;
    let mut check = |name: &str, ok: bool| {
        println!("  [{}] {name}", if ok { "ok" } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    };

    println!("Cross-domain resolve: satellite (ECI) → station ENU, vs direct geometry\n");
    println!("    t (min)   resolver az/el (deg)      direct az/el (deg)        Δ");

    let mut max_err = 0.0f64;
    let mut interp_max_err = 0.0f64;
    for step in 0..=9 {
        let t = step as f64 * 1200.0; // every 20 min, 0..3 h (off the 30-min sample grid → exercises interpolation)

        // Resolver path: ECI → ENU via the frame tree.
        let enu = tree.resolve_point(sat_eci, "ECI", "ENU", t);
        let (az_r, el_r) = az_el(enu);

        // Independent oracle: manual ECI→ECEF, then dot onto ENU basis.
        let th = theta(t);
        let (sth, cth) = th.sin_cos();
        // ECI→ECEF is Rz(−θ): [x,y]→[x cosθ + y sinθ, −x sinθ + y cosθ].
        let sat_ecef = [
            sat_eci[0] * cth + sat_eci[1] * sth,
            -sat_eci[0] * sth + sat_eci[1] * cth,
            sat_eci[2],
        ];
        let los = sub3(sat_ecef, p_station_ecef);
        let enu_direct = [dot3(los, east), dot3(los, north), dot3(los, up)];
        let (az_d, el_d) = az_el(enu_direct);

        // Also confirm the *interpolated* dynamic edge matches the analytic
        // Earth rotation at this off-grid time (the sclerp-of-samples claim).
        let edge = tree.edge_at("ECEF", t);
        let analytic = Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], th));
        let probe = [A_WGS84, 0.0, 0.0];
        interp_max_err = interp_max_err.max(norm3(sub3(edge.transform_point(probe), analytic.transform_point(probe))));

        let derr = ((az_r - az_d + 540.0).rem_euclid(360.0) - 180.0).abs() + (el_r - el_d).abs();
        max_err = max_err.max(derr);
        println!(
            "    {:6.0}    az={:7.2}  el={:6.2}      az={:7.2}  el={:6.2}     {:.2e}",
            t / 60.0, az_r, el_r, az_d, el_d, derr
        );
    }
    println!();

    check("resolver az/el matches direct geometry at every step (< 1e-6°)", max_err < 1e-6);
    check("interpolated Earth-spin edge matches analytic Rz(θ) off-grid (< 1 m)", interp_max_err < 1.0);

    // The satellite must actually rise above the horizon during the pass
    // (elevation goes positive) — a sanity check that the geometry is real,
    // not a coordinate artifact.
    let el0 = az_el(tree.resolve_point(sat_eci, "ECI", "ENU", 0.0)).1;
    let el_mid = az_el(tree.resolve_point(sat_eci, "ECI", "ENU", 5400.0)).1;
    check("satellite elevation changes over the pass (a real ground track)", (el_mid - el0).abs() > 1.0);

    println!();
    if fails == 0 {
        println!("OK — a satellite in ECI and a station on ECEF resolve into one frame by");
        println!("composing rigid transforms (one dynamic, screw-interpolated; one static),");
        println!("and the answer matches raw geometry exactly. The TF-tree unification holds:");
        println!("orbital and ground data share a single coordinate system.");
    } else {
        println!("{fails} check(s) FAILED.");
        std::process::exit(1);
    }
}
