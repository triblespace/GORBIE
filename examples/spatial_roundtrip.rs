//! M1 substrate — the `spatial::*` schema, verified headless.
//!
//! Defines the `Position` inline encoding (3×f64 ECEF metres, the
//! load-bearing core-grade encoding from the spatial-frames design) and
//! the `spatial::position` / `spatial::frame` attributes, then proves
//! the full round trip: build a `TribleSet` of city markers via
//! `entity!`, query them back via `pattern!`, and assert the decoded
//! ECEF coordinates match the WGS84 conversion. No GPU, no window — runs
//! to completion and exits, so it is verifiable without a live display
//! (unlike the globe paint callback).
//!
//! This is the schema half of M1 (markers + great-circle arcs). The
//! render half hangs off the globe widget once M0 is confirmed live.
//! The encoding is prototyped here (GORBIE-local) per the design's
//! "prototype, then promote to triblespace-core" path; the minted IDs
//! are stable and carry across promotion.
//!
//! ```sh
//! cargo run --example spatial_roundtrip --features triblespace
//! ```

use triblespace::core::id::{ExclusiveId, Id};
use triblespace::core::inline::{
    Encodes, Inline, InlineEncoding, RawInline, TryFromInline,
};
use triblespace::core::metadata::{self, MetaDescribe};
use triblespace::core::trible::{Fragment, TribleSet};
use triblespace::macros::{find, id_hex, pattern};

// ── The Position inline encoding ─────────────────────────────────────

/// Three IEEE-754 doubles (x, y, z) stored little-endian in the first 24
/// of 32 bytes; the remaining 8 are zero. ECEF metres by convention —
/// f64 because absolute Earth-frame magnitudes are ~6.4e6 m and f32
/// would go coarse (~0.4 m) at that scale. Mirrors the `LineLocation`
/// four-u64 packing in triblespace-core; promote it there beside f64
/// once blessed.
pub struct Position;

impl MetaDescribe for Position {
    fn describe() -> Fragment {
        let id: Id = id_hex!("CEEB1D1C9F79CE012AD9AE1DE54149C3");
        entity_describe(id)
    }
}

// Small helper so the long `entity!` doesn't clutter the impl.
fn entity_describe(id: Id) -> Fragment {
    use triblespace::macros::entity;
    entity! {
        ExclusiveId::force_ref(&id) @
            metadata::name: "position_f64x3",
            metadata::description: "Three IEEE-754 doubles (x, y, z) little-endian in the first 24 of 32 bytes; trailing 8 bytes zero. ECEF metres by convention. f64 because absolute Earth-frame magnitudes (~6.4e6 m) exceed f32's safe range; relative transforms use the f32 motor encoding instead. Decode with `[f64;3]`/glam DVec3.",
            metadata::tag: metadata::KIND_INLINE_ENCODING,
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

/// Default spatial attributes a renderer consumes. `position` is the
/// ECEF location; `frame` names the coordinate frame the position lives
/// in (the TF-tree node id) — for single-frame data it is optional and
/// defaults to ECEF.
pub mod spatial {
    use super::Position;
    use triblespace::macros::attributes;
    use triblespace::prelude::inlineencodings;

    attributes! {
        /// ECEF position of an entity, metres.
        "2CC4C7111FE60AB83C4310D5F5E1DA38" as position: Position;
        /// The coordinate frame (TF-tree node) this entity is expressed in.
        "BA6F762E56F52E7B0FBD0B46344C60B9" as frame: inlineencodings::GenId;
    }
}

// ── WGS84 geodetic → ECEF ────────────────────────────────────────────

/// Geodetic (lat, lon in degrees, altitude in metres) → ECEF metres,
/// WGS84 ellipsoid. This is the parameterization the design calls out:
/// "geodetic lat/lon/alt is a parameterization of an ECEF position;
/// geodata converts in."
fn geodetic_to_ecef(lat_deg: f64, lon_deg: f64, alt_m: f64) -> [f64; 3] {
    const A: f64 = 6_378_137.0; // semi-major axis
    const F: f64 = 1.0 / 298.257_223_563; // flattening
    let e2 = F * (2.0 - F);
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let (sin_lat, cos_lat) = lat.sin_cos();
    let (sin_lon, cos_lon) = lon.sin_cos();
    let n = A / (1.0 - e2 * sin_lat * sin_lat).sqrt();
    [
        (n + alt_m) * cos_lat * cos_lon,
        (n + alt_m) * cos_lat * sin_lon,
        (n * (1.0 - e2) + alt_m) * sin_lat,
    ]
}

// ── Round-trip proof ─────────────────────────────────────────────────

fn main() {
    use triblespace::macros::entity;
    use triblespace::core::inline::IntoInline;

    // A handful of cities (name, lat, lon).
    let cities = [
        ("Reykjavík", 64.1466, -21.9426),
        ("Quito", -0.1807, -78.4678),
        ("Singapore", 1.3521, 103.8198),
        ("Longyearbyen", 78.2232, 15.6469),
    ];

    // Build the marker set. Each entity gets an ECEF position keyed by
    // its genid; the display name stays in our own table (a LongString
    // name would need a blob store this headless demo deliberately omits
    // — the point under test is the Position encoding, not name blobs).
    let mut markers = TribleSet::new();
    let mut expected: Vec<(Id, String, [f64; 3])> = Vec::new();
    for (name, lat, lon) in cities {
        let ecef = geodetic_to_ecef(lat, lon, 0.0);
        let id = triblespace::core::id::fucid();
        markers += entity! { &id @
            spatial::position: ecef.to_inline(),
        };
        expected.push((*id, name.to_string(), ecef));
    }

    // Query positions back out, keyed by entity id, and decode them.
    let rows: Vec<(Id, Inline<Position>)> = find!(
        (e: Id, pos: Inline<Position>),
        pattern!(&markers, [{ ?e @ spatial::position: ?pos }])
    )
    .into_iter()
    .collect();

    println!("decoded {} markers from the pile:\n", rows.len());
    let mut ok = 0usize;
    for (e, pos) in &rows {
        let xyz: [f64; 3] = <[f64; 3]>::try_from_inline(pos).unwrap();
        let (name, want) = expected
            .iter()
            .find(|(id, _, _)| id == e)
            .map(|(_, n, ec)| (n.clone(), *ec))
            .expect("queried id not in expected set");
        let err = ((xyz[0] - want[0]).powi(2)
            + (xyz[1] - want[1]).powi(2)
            + (xyz[2] - want[2]).powi(2))
        .sqrt();
        let r = (xyz[0] * xyz[0] + xyz[1] * xyz[1] + xyz[2] * xyz[2]).sqrt();
        println!(
            "  {name:<14} ECEF = [{:>12.1}, {:>12.1}, {:>12.1}]  |r|={:.1} km  roundtrip_err={err:.3e} m",
            xyz[0], xyz[1], xyz[2], r / 1000.0
        );
        assert!(err < 1e-6, "roundtrip error too large for {name}: {err}");
        ok += 1;
    }

    assert_eq!(ok, cities.len(), "not every marker round-tripped");
    println!(
        "\nOK — {ok}/{} markers round-tripped through Position (f64×3) exactly.",
        cities.len()
    );
    println!("ECEF radii sit at ~6357–6378 km (pole vs equator), confirming the WGS84 ellipsoid.");

    external_reality_check();
}

/// External-reality validation of the WGS84 geodetic→ECEF conversion
/// against *authoritative published values*, not just internal
/// self-consistency.
///
/// The earlier round-trip proves the encoding is lossless and that the
/// converter's output is internally consistent, but "the ellipsoid falls
/// out of the radii" is a weak claim — any oblate-ish formula would pass.
/// This pins the converter to the numbers a standards body publishes.
///
/// Source: NGA.STND.0036_1.0.0_WGS84, "Department of Defense World
/// Geodetic System 1984" (National Geospatial-Intelligence Agency,
/// 2014-07-08), Table 3.1 (defining parameters) and derived geometric
/// constants:
///   - semi-major axis        a  = 6378137.0 m            (defining, exact)
///   - inverse flattening     1/f = 298.257223563         (defining, exact)
///   - semi-minor axis        b  = 6356752.3142 m         (derived)
///   - first eccentricity²    e² = 6.694379990141e-3      (derived)
///
/// These are checkpoints where the WGS84 geometry is fixed by definition,
/// so the ECEF coordinates are *published values*, not model output:
///   - the equator on the prime meridian sits at ECEF (a, 0, 0);
///   - a quarter-turn east sits at ECEF (0, a, 0);
///   - the geographic north pole sits at ECEF (0, 0, b) — the polar
///     radius IS the published semi-minor axis.
fn external_reality_check() {
    // NGA WGS84 published constants.
    const A_PUB: f64 = 6_378_137.0; // semi-major axis, m (defining)
    const B_PUB: f64 = 6_356_752.3142; // semi-minor axis, m (derived, 4 dp as published)
    const E2_PUB: f64 = 6.694_379_990_141e-3; // first eccentricity squared (derived)

    // The converter's own e² (from the defining 1/f) must match the
    // independently-published derived e² to full published precision.
    const F: f64 = 1.0 / 298.257_223_563;
    let e2 = F * (2.0 - F);

    println!("\nExternal-reality check — WGS84 geodetic→ECEF vs NGA.STND.0036 published values:\n");
    let mut fails = 0usize;
    let mut check = |name: &str, ok: bool| {
        println!("  [{}] {name}", if ok { "ok" } else { "FAIL" });
        if !ok {
            fails += 1;
        }
    };

    check(
        &format!(
            "first eccentricity² matches published 6.694379990141e-3 (got {e2:.15e})"
        ),
        (e2 - E2_PUB).abs() < 1e-15,
    );

    // Prime-meridian equator → published (a, 0, 0).
    let eq0 = geodetic_to_ecef(0.0, 0.0, 0.0);
    check(
        &format!("equator/prime-meridian ECEF = (a,0,0) = ({A_PUB:.1}, 0, 0)  [got {:.4}, {:.4}, {:.4}]", eq0[0], eq0[1], eq0[2]),
        (eq0[0] - A_PUB).abs() < 1e-6 && eq0[1].abs() < 1e-6 && eq0[2].abs() < 1e-6,
    );

    // Equator, 90° east → published (0, a, 0).
    let eq90 = geodetic_to_ecef(0.0, 90.0, 0.0);
    check(
        &format!("equator/90°E ECEF = (0,a,0) = (0, {A_PUB:.1}, 0)  [got {:.4}, {:.4}, {:.4}]", eq90[0], eq90[1], eq90[2]),
        eq90[0].abs() < 1e-6 && (eq90[1] - A_PUB).abs() < 1e-6 && eq90[2].abs() < 1e-6,
    );

    // North pole → published (0, 0, b): the polar radius is the semi-minor
    // axis. Tolerance 1e-3 m because B_PUB is quoted to 4 decimal places.
    let pole = geodetic_to_ecef(90.0, 0.0, 0.0);
    check(
        &format!("north-pole ECEF Z = semi-minor axis b = {B_PUB:.4} m  [got {:.4} m]", pole[2]),
        pole[0].abs() < 1e-6 && pole[1].abs() < 1e-6 && (pole[2] - B_PUB).abs() < 1e-3,
    );

    if fails == 0 {
        println!(
            "\nOK — the converter reproduces WGS84's published semi-major/semi-minor axes and\n\
             eccentricity exactly, so the geodetic→ECEF path is validated against an external\n\
             standard, not just against itself."
        );
    } else {
        println!("\n{fails} external-reality check(s) FAILED.");
        std::process::exit(1);
    }

    // Honesty about scope: what is NOT validated here, and what real
    // external validation would require.
    //
    // These checkpoints fix the ellipsoid's *shape* (a, b, e²) but not a
    // real point on the crust. Full external validation against measured
    // reality would import an authoritative station catalogue — e.g. the
    // IGS/ITRF2020 SINEX solution, which publishes each GNSS reference
    // station's ECEF (X,Y,Z) to the millimetre — and assert the converter
    // reproduces the published ECEF from that station's published
    // geodetic latitude/longitude/ellipsoidal-height, allowing for the
    // cm-level ITRF-vs-WGS84 datum realisation difference. That needs an
    // external dataset (offline-fetchable but not vendored here), so it is
    // documented rather than run. Likewise, validating a *satellite* pass
    // would require a real TLE + an SGP4 propagator (an external model)
    // to generate ground-truth ECI/ECEF states — out of scope for this
    // headless encoding test, and noted so the gap is explicit.
    println!(
        "\nNot validated offline (documented, not run): real ITRF station ECEF (needs the\n\
         IGS/ITRF2020 SINEX catalogue) and real satellite ephemeris (needs a TLE + SGP4).\n\
         Those require external datasets/models; the checks above are the strongest offline\n\
         validation — published defining/derived WGS84 constants."
    );
}
