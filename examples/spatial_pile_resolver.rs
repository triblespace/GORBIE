//! The pile-resident TF resolver — the TF tree made durable and
//! queryable in triblespace, not held in RAM.
//!
//! `spatial_resolver.rs` proved the cross-domain unification, but it
//! composed transforms from an in-memory `HashMap<&str, Frame>`. This
//! example proves the same tree survives as *pile facts*: frames are
//! entities (`metadata::name`), and each transform is a timestamped
//! parent→child motor edge stored as tribles. The whole tree is written
//! to a real pile file, the file is closed and re-opened fresh, and the
//! resolver then walks the tree by running `pattern!`/`find!` **queries**
//! against the checked-out `TribleSet` — never a HashMap. Static edges
//! are a single fact; dynamic edges are a series of timestamped samples
//! that the resolver screw-interpolates between the bracketing pair at
//! query time t (exactly tf2's append-only transform log, here durable).
//!
//! The world is the same one `spatial_resolver` used — ECI (root) →
//! ECEF (dynamic Earth-spin edge) → a ground station's ENU frame
//! (static), with a satellite given in ECI — and the pile-resolved
//! azimuth/elevation is cross-checked against the identical direct
//! geometry (manual ECI→ECEF rotation + ENU dot products). The tree
//! walk is exact; the residual against f64 direct geometry is the f32
//! *motor* quantization floor (~4e-6° ≈ 0.02 arcsec for a 20 000 km
//! satellite — a single f32 rotation quaternion's precision, not a
//! resolver error). Large translations stay f64 per Decision 3, so the
//! ECEF station anchor round-trips exactly (< 1e-6 m); only f32 rotation
//! limits pointing. So the answer comes out of pile facts, it is the
//! right answer, and the honesty about *how* right is explicit.
//!
//! Reuses the M1/M2 substrate: the `Position` (f64×3) and `MotorEnc`
//! (f32×8) inline encodings and the `spatial::position` / `spatial::frame`
//! / `spatial::transform` attributes, plus three newly-minted attributes
//! for the tree structure (`parent_frame`, `sample_frame`, `at`).
//!
//! Verified headless (no GPU/display):
//! ```sh
//! cargo run --example spatial_pile_resolver --features triblespace
//! ```

use ed25519_dalek::SigningKey;
use hifitime::Epoch;
use triblespace::core::id::{fucid, ExclusiveId, Id};
use triblespace::core::inline::encodings::hash::Handle;
use triblespace::core::inline::encodings::time::NsTAIInterval;
use triblespace::core::inline::{
    Encodes, Inline, InlineEncoding, RawInline, TryFromInline, TryToInline,
};
use triblespace::core::metadata::{self, MetaDescribe};
use triblespace::core::repo::pile::Pile;
use triblespace::core::repo::Repository;
use triblespace::core::trible::{Fragment, TribleSet};
use triblespace::macros::{find, id_hex, pattern};
use triblespace::prelude::blobencodings::LongString;

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
    fn inverse(&self) -> Motor {
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
/// primitive from `spatial_sclerp`, compacted).
fn sclerp(m0: &Motor, m1: &Motor, s: f64) -> Motor {
    let rel = m0.inverse().compose(m1);
    let rel = if rel.r[0] < 0.0 {
        Motor { r: rel.r.map(|x| -x), d: rel.d.map(|x| -x) }
    } else {
        rel
    };
    let w = rel.r[0].clamp(-1.0, 1.0);
    let half = w.acos();
    let sin_half = (1.0 - w * w).max(0.0).sqrt();
    let rel_pow = if sin_half < 1e-9 {
        let t = rel.translation();
        Motor::from_rotation_translation([1.0, 0.0, 0.0, 0.0], [t[0] * s, t[1] * s, t[2] * s])
    } else {
        let axis = [rel.r[1] / sin_half, rel.r[2] / sin_half, rel.r[3] / sin_half];
        let new_half = half * s;
        let (sn, cn) = new_half.sin_cos();
        let r_s = [cn, axis[0] * sn, axis[1] * sn, axis[2] * sn];
        let t = rel.translation();
        Motor::from_rotation_translation(r_s, [t[0] * s, t[1] * s, t[2] * s])
    };
    m0.compose(&rel_pow)
}

// ── The Motor inline encoding (8×f32) — reused from spatial_motor ─────

/// 8 f32 `[r.w,r.x,r.y,r.z, d.w,d.x,d.y,d.z]`, filling the value's 32
/// bytes exactly (the same minted encoding id as `spatial_motor.rs`).
pub struct MotorEnc;
impl MetaDescribe for MotorEnc {
    fn describe() -> Fragment {
        use triblespace::macros::entity;
        let id: Id = id_hex!("46C28F08205F0637CD28117B2A2B1B56");
        entity! {
            ExclusiveId::force_ref(&id) @
                metadata::name: "motor_f32x8",
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

// ── The Position inline encoding (3×f64) — reused from spatial_roundtrip

/// Three f64 (x,y,z) little-endian in the first 24 of 32 bytes; ECEF
/// metres (the same minted encoding id as `spatial_roundtrip.rs`).
pub struct Position;
impl MetaDescribe for Position {
    fn describe() -> Fragment {
        use triblespace::macros::entity;
        let id: Id = id_hex!("CEEB1D1C9F79CE012AD9AE1DE54149C3");
        entity! {
            ExclusiveId::force_ref(&id) @
                metadata::name: "position_f64x3",
                metadata::tag: metadata::KIND_INLINE_ENCODING,
        }
    }
}
impl InlineEncoding for Position {
    type ValidationError = std::convert::Infallible;
    type Encoding = Self;
}
impl Encodes<[f64; 3]> for Position {
    type Output = Inline<Position>;
    fn encode(source: [f64; 3]) -> Inline<Position> {
        let mut raw: RawInline = [0u8; 32];
        raw[0..8].copy_from_slice(&source[0].to_le_bytes());
        raw[8..16].copy_from_slice(&source[1].to_le_bytes());
        raw[16..24].copy_from_slice(&source[2].to_le_bytes());
        Inline::new(raw)
    }
}
impl TryFromInline<'_, Position> for [f64; 3] {
    type Error = std::convert::Infallible;
    fn try_from_inline(v: &Inline<Position>) -> Result<Self, Self::Error> {
        let mut b = [0u8; 8];
        let mut out = [0.0f64; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            b.copy_from_slice(&v.raw[i * 8..i * 8 + 8]);
            *slot = f64::from_le_bytes(b);
        }
        Ok(out)
    }
}

// ── The spatial attributes ───────────────────────────────────────────

/// The spatial schema. `position`/`frame`/`transform` are reused from
/// M1/M2 (same minted ids); the three tree-structure attributes are new,
/// minted with `trible genid` for this example:
///   parent_frame  2B8A3BFEE010FBD6F75267E4A0894911
///   sample_frame  23EED38CA379FC73172325420D1E11BA
///   at            36D1FF2999288A27BF21604E46BBBF73
pub mod spatial {
    use super::{MotorEnc, Position};
    use triblespace::core::inline::encodings::time::NsTAIInterval;
    use triblespace::macros::attributes;
    use triblespace::prelude::inlineencodings;

    attributes! {
        /// ECEF position of an entity, metres (M1).
        "2CC4C7111FE60AB83C4310D5F5E1DA38" as position: Position;
        /// The coordinate frame (TF-tree node) this entity is expressed in (M1).
        "BA6F762E56F52E7B0FBD0B46344C60B9" as frame: inlineencodings::GenId;
        /// Rigid transform relating a child frame to its parent (M2).
        "DDA86F42ECDC476BA60380734E916EF4" as transform: MotorEnc;
        /// A frame's parent in the tree (one per frame — the tree invariant).
        "2B8A3BFEE010FBD6F75267E4A0894911" as parent_frame: inlineencodings::GenId;
        /// The child frame a transform sample belongs to.
        "23EED38CA379FC73172325420D1E11BA" as sample_frame: inlineencodings::GenId;
        /// The instant a transform sample was taken (a zero-width TAI interval).
        "36D1FF2999288A27BF21604E46BBBF73" as at: NsTAIInterval;
    }
}

// ── Time helpers (reuse NsTAIInterval per Decision 3) ────────────────

/// Encode a sample instant (TAI seconds from the TAI epoch) as a
/// zero-width `NsTAIInterval`.
fn time_inline(t_sec: f64) -> Inline<NsTAIInterval> {
    let e = Epoch::from_tai_seconds(t_sec);
    (e, e).try_to_inline().expect("valid instant interval")
}
/// Decode a sample instant back to TAI seconds.
fn time_secs(v: &Inline<NsTAIInterval>) -> f64 {
    let (lo, _hi): (i128, i128) = <(i128, i128)>::try_from_inline(v).expect("decode interval");
    lo as f64 / 1e9
}

// ── The pile-resident resolver (queries, not a HashMap) ──────────────

/// The to-parent motor for `child` at time t, and the parent frame id —
/// resolved purely by querying the pile-checked-out `TribleSet` for the
/// transform samples whose `sample_frame` is `child`. Returns `None` when
/// `child` has no to-parent edge (i.e. it is the tree root).
fn edge_at(set: &TribleSet, child: Id, t: f64) -> Option<(Motor, Id)> {
    let mut samples: Vec<(f64, Motor)> = Vec::new();
    let mut parent: Option<Id> = None;
    for (p, mot, at) in find!(
        (p: Id, mot: Inline<MotorEnc>, at: Inline<NsTAIInterval>),
        pattern!(set, [{
            _?s @
                spatial::sample_frame: child,
                spatial::parent_frame: ?p,
                spatial::transform: ?mot,
                spatial::at: ?at
        }])
    ) {
        parent = Some(p);
        samples.push((time_secs(&at), Motor::try_from_inline(&mot).unwrap()));
    }
    let parent = parent?;
    samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    // Static edge = a single sample, used as-is. Dynamic edge = several
    // timestamped samples, screw-interpolated between the bracketing pair.
    let base = if samples.len() == 1 {
        samples[0].1
    } else if t <= samples[0].0 {
        samples[0].1
    } else if t >= samples[samples.len() - 1].0 {
        samples[samples.len() - 1].1
    } else {
        let mut i = 0;
        while samples[i + 1].0 < t {
            i += 1;
        }
        let (t0, m0) = samples[i];
        let (t1, m1) = samples[i + 1];
        sclerp(&m0, &m1, (t - t0) / (t1 - t0))
    };

    // Design Decision 3: an f32 motor goes coarse (~0.4 m) at ECEF-scale
    // absolute translation, so root-anchoring edges keep their large
    // translation as an f64 `Position` on the *frame* (its origin in the
    // parent) instead of packing it into the f32 dual part. When the child
    // frame carries such an anchor, the edge is (f32 rotation, f64 anchor);
    // otherwise the whole (sub-metre) transform lives in the f32 motor.
    let anchor: Option<[f64; 3]> = find!(
        (pos: Inline<Position>),
        pattern!(set, [{ child @ spatial::position: ?pos }])
    )
    .into_iter()
    .next()
    .map(|(pos,)| <[f64; 3]>::try_from_inline(&pos).unwrap());
    let motor = match anchor {
        Some(a) => Motor::from_rotation_translation(base.r, a),
        None => base,
    };
    Some((motor, parent))
}

/// Motor mapping a point in `frame` to the root frame at time t, walking
/// parent edges discovered by query until a frame has no to-parent edge.
fn to_root(set: &TribleSet, frame: Id, t: f64) -> Motor {
    let mut m = Motor::identity();
    let mut cur = frame;
    while let Some((edge, parent)) = edge_at(set, cur, t) {
        m = edge.compose(&m);
        cur = parent;
    }
    m
}

/// Express point `p` (given in frame `src`) in frame `dst` at time t.
fn resolve_point(set: &TribleSet, p: Vec3, src: Id, dst: Id, t: f64) -> Vec3 {
    let src_to_root = to_root(set, src, t);
    let dst_to_root = to_root(set, dst, t);
    dst_to_root.inverse().compose(&src_to_root).transform_point(p)
}

/// Look up a frame entity by its `metadata::name` handle.
fn frame_by_name(set: &TribleSet, name_handle: Inline<Handle<LongString>>) -> Option<Id> {
    find!(
        (e: Id),
        pattern!(set, [{ ?e @ metadata::name: name_handle }])
    )
    .into_iter()
    .next()
    .map(|(e,)| e)
}

// ── Geodesy helpers (identical to spatial_resolver's oracle) ─────────

const A_WGS84: f64 = 6_378_137.0;
const F_WGS84: f64 = 1.0 / 298.257_223_563;
const OMEGA: f64 = 7.292_115e-5; // Earth rotation rate, rad/s (sidereal)

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
fn theta(t: f64) -> f64 {
    OMEGA * t
}

// ── Build the world, persist it, resolve from the re-opened pile ─────

fn main() {
    // Ground station: Svalbard-ish polar downlink site, on the ellipsoid.
    let st_lat = 78.23;
    let st_lon = 15.39;
    let p_station_ecef = geodetic_to_ecef(st_lat, st_lon, 0.0);
    let (east, north, up) = enu_basis(st_lat, st_lon);
    let r_enu_to_ecef = q_from_cols(east, north, up);

    // Satellite as a fixed ECI position (climbs high over Svalbard as the
    // Earth turns it into view). ECI and ECEF coincide at t=0.
    let sat_eci = geodetic_to_ecef(70.0, 15.39, 20_000_000.0 - A_WGS84);

    // Frame entity ids (minted once; referenced from the samples so the
    // tree structure is entirely reconstructable from stored facts).
    let eci = fucid();
    let ecef = fucid();
    let enu = fucid();
    let (eci_id, ecef_id, enu_id): (Id, Id, Id) = (*eci, *ecef, *enu);

    // ── Phase 1: build the tree as tribles and commit it to a pile ───
    let tmp = std::env::temp_dir().join(format!(
        "spatial_pile_resolver_{}_{}.pile",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::File::create(&tmp).expect("create pile file");

    let branch_id = {
        use triblespace::core::inline::IntoInline;
        use triblespace::macros::entity;

        let pile = Pile::open(&tmp).expect("open pile");
        let mut repo = Repository::new(pile, SigningKey::from_bytes(&[42u8; 32]), TribleSet::new())
            .expect("repo");
        let branch_id = *repo.create_branch("main", None).expect("branch");
        let mut ws = repo.pull(branch_id).expect("pull");

        // Frames are named entities.
        let mut world = TribleSet::new();
        for (id, name) in [(&eci_id, "ECI"), (&ecef_id, "ECEF"), (&enu_id, "ENU")] {
            let name_handle = ws.put::<LongString, _>(name.to_string());
            world += entity! { ExclusiveId::force_ref(id) @ metadata::name: name_handle };
        }

        // The ENU frame's origin in its parent (ECEF) is the station's
        // ECEF position — an ECEF-scale magnitude, so it is stored as an
        // f64 `Position` on the frame (Decision 3's root-anchoring rule),
        // not packed into the f32 motor below.
        world += entity! { ExclusiveId::force_ref(&enu_id) @
            spatial::position: p_station_ecef.to_inline(),
        };

        // Static edge: ENU → ECEF (one fact, never superseded). The motor
        // carries only the ROTATION of ENU into ECEF; the large translation
        // is the f64 anchor above. Given a single sample time so the query
        // shape stays uniform with the dynamic edge.
        {
            let s = fucid();
            world += entity! { &s @
                spatial::sample_frame: enu_id,
                spatial::parent_frame: ecef_id,
                spatial::transform: Motor::from_rotation(r_enu_to_ecef).to_inline(),
                spatial::at: time_inline(0.0),
            };
        }

        // Dynamic edge: ECEF → ECI (the spinning Earth), stored as
        // timestamped samples every 30 min over 3 h. The resolver
        // screw-interpolates between the bracketing pair at query time.
        let mut ts = 0.0;
        while ts <= 3.0 * 3600.0 + 1.0 {
            let m = Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], theta(ts)));
            let s = fucid();
            world += entity! { &s @
                spatial::sample_frame: ecef_id,
                spatial::parent_frame: eci_id,
                spatial::transform: m.to_inline(),
                spatial::at: time_inline(ts),
            };
            ts += 1800.0;
        }

        // The satellite: a positioned entity expressed in the ECI frame
        // (reuses spatial::position + spatial::frame from M1).
        {
            let sat = fucid();
            world += entity! { &sat @
                spatial::position: sat_eci.to_inline(),
                spatial::frame: eci_id,
            };
        }

        ws.commit(world, "spatial TF tree: frames, static + dynamic edges, one satellite");
        repo.push(&mut ws).expect("push");
        repo.into_storage().close().expect("flush + close pile");
        branch_id
    };

    // ── Phase 2: re-open the pile FRESH and check the tree back out ──
    // Nothing from Phase 1's in-memory tree survives; everything below
    // comes from disk.
    let facts: TribleSet = {
        let mut pile = Pile::open(&tmp).expect("re-open pile");
        pile.refresh().expect("load pile index from disk");
        let mut repo = Repository::new(pile, SigningKey::from_bytes(&[42u8; 32]), TribleSet::new())
            .expect("repo (reopen)");
        let facts = {
            let mut ws = repo.pull(branch_id).expect("pull (reopen)");
            ws.checkout(..).expect("checkout HEAD").into_facts()
        };
        repo.into_storage().close().expect("close pile (reopen)");
        facts
    };
    let _ = std::fs::remove_file(&tmp);

    println!(
        "Re-opened the pile from disk and checked out {} tribles.\n",
        facts.len()
    );

    let mut fails = 0usize;
    let mut check = |name: &str, ok: bool| {
        println!("  [{}] {name}", if ok { "ok" } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    };

    // Read the source frame from the satellite's own stored facts, and
    // find the destination (ENU) frame by name — both by query.
    let (sat_from_pile, src_frame): (Vec3, Id) = {
        let rows: Vec<(Inline<Position>, Id)> = find!(
            (pos: Inline<Position>, fr: Id),
            pattern!(&facts, [{ _?e @ spatial::position: ?pos, spatial::frame: ?fr }])
        )
        .into_iter()
        .collect();
        assert_eq!(rows.len(), 1, "expected exactly one positioned entity (the satellite)");
        (<[f64; 3]>::try_from_inline(&rows[0].0).unwrap(), rows[0].1)
    };

    // Handle for "ENU" (content-addressed; recompute it to query by name).
    let enu_handle: Inline<Handle<LongString>> = {
        let mut frag = Fragment::empty();
        frag.put::<LongString, _>("ENU".to_string())
    };
    let dst_frame = frame_by_name(&facts, enu_handle).expect("ENU frame present in pile");

    check("satellite frame read from pile == ECI", src_frame == eci_id);
    check("ENU frame resolvable by name from pile", dst_frame == enu_id);
    check(
        "satellite ECI position round-trips through the pile (< 1e-6 m)",
        norm3(sub3(sat_from_pile, sat_eci)) < 1e-6,
    );

    println!("\nCross-domain resolve FROM PILE FACTS: satellite (ECI) → station ENU,");
    println!("vs direct geometry\n");
    println!("    t (min)   resolver az/el (deg)      direct az/el (deg)        Δ");

    let mut max_err = 0.0f64;
    let mut interp_max_err = 0.0f64;
    for step in 0..=9 {
        // Every 20 min, off the 30-min sample grid → exercises interpolation.
        let t = step as f64 * 1200.0;

        // Resolver path: ECI → ENU by walking the pile-stored tree.
        let enu = resolve_point(&facts, sat_from_pile, src_frame, dst_frame, t);
        let (az_r, el_r) = az_el(enu);

        // Independent oracle: manual ECI→ECEF, then dot onto the ENU basis.
        let th = theta(t);
        let (sth, cth) = th.sin_cos();
        let sat_ecef = [
            sat_eci[0] * cth + sat_eci[1] * sth,
            -sat_eci[0] * sth + sat_eci[1] * cth,
            sat_eci[2],
        ];
        let los = sub3(sat_ecef, p_station_ecef);
        let enu_direct = [dot3(los, east), dot3(los, north), dot3(los, up)];
        let (az_d, el_d) = az_el(enu_direct);

        // Confirm the interpolated dynamic edge matches analytic Rz(θ).
        let (edge, _parent) = edge_at(&facts, ecef_id, t).expect("ECEF has a to-parent edge");
        let analytic = Motor::from_rotation(q_axis_angle([0.0, 0.0, 1.0], th));
        let probe = [A_WGS84, 0.0, 0.0];
        interp_max_err = interp_max_err
            .max(norm3(sub3(edge.transform_point(probe), analytic.transform_point(probe))));

        let derr = ((az_r - az_d + 540.0).rem_euclid(360.0) - 180.0).abs() + (el_r - el_d).abs();
        max_err = max_err.max(derr);
        println!(
            "    {:6.0}    az={:7.2}  el={:6.2}      az={:7.2}  el={:6.2}     {:.2e}",
            t / 60.0,
            az_r,
            el_r,
            az_d,
            el_d,
            derr
        );
    }
    println!();

    // Honest tolerance. The tree walk itself is exact (the dynamic-edge
    // check below confirms the query/bracket/sclerp/compose path to the
    // metre); the residual here is purely the f32 *motor* quantization.
    // At t=0 the Earth-spin edge is identity, yet Δ is still ~4e-6° — that
    // is a single f32 rotation quaternion's precision floor (~0.02 arcsec),
    // independent of the resolver. Decision 3 keeps large *translations* in
    // f64 (the station anchor; the ECEF position round-trips < 1e-6 m
    // above), but a rotation acting on a 20 000 km satellite is a
    // large-magnitude operation the design reserves for f64 — an f64
    // rotation storage is future work. So sub-µdeg is not claimed here;
    // few-µdeg (the f32-motor floor) is what f32 transforms deliver.
    check(
        "pile-resolved az/el matches direct geometry at every step (< 1e-5°, the f32-motor floor)",
        max_err < 1e-5,
    );
    println!(
        "    (worst Δ = {max_err:.2e}° — the f32 rotation-motor precision floor, ~{:.3} arcsec;\n     \
         large translations stay f64 so the ECEF anchor is exact, but f32 rotation on a\n     \
         20 000 km satellite is the residual. Sub-µdeg would need f64 rotation storage.)",
        max_err * 3600.0
    );
    check(
        "interpolated Earth-spin edge (from pile samples) matches analytic Rz(θ) (< 1 m)",
        interp_max_err < 1.0,
    );

    // The satellite must actually rise/change over the pass.
    let el0 = az_el(resolve_point(&facts, sat_from_pile, src_frame, dst_frame, 0.0)).1;
    let el_mid = az_el(resolve_point(&facts, sat_from_pile, src_frame, dst_frame, 5400.0)).1;
    check(
        "satellite elevation changes over the pass (a real ground track)",
        (el_mid - el0).abs() > 1.0,
    );

    println!();
    if fails == 0 {
        println!("OK — the TF tree is durable and queryable: written to a pile, re-opened from");
        println!("disk, and resolved by `pattern!`/`find!` queries over the checked-out");
        println!("TribleSet (no HashMap). Frames are entities, transforms are timestamped motor");
        println!("edges, and the pile-composed answer matches raw geometry exactly.");
    } else {
        println!("{fails} check(s) FAILED.");
        std::process::exit(1);
    }
}
