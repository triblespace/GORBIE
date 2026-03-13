use std::collections::HashMap;

use eframe::egui;
use egui::epaint;

/// Cached tessellated glyph meshes.
pub struct GlyphCache {
    /// Keyed by (font data pointer, glyph ID) → pre-triangulated glyph.
    meshes: HashMap<(usize, u16), GlyphMesh>,
}

/// A pre-triangulated glyph stored at unit scale (1 unit = 1 font design unit).
/// At render time, vertices are scaled by (font_size / units_per_em).
pub struct GlyphMesh {
    /// Vertices in font coordinates (Y-up).
    pub vertices: Vec<[f32; 2]>,
    /// Triangle indices into `vertices`.
    pub triangles: Vec<[u32; 3]>,
    /// Boundary contour loops as vertex index sequences (closed).
    /// Used at render time to add anti-aliasing feathering.
    pub boundary_loops: Vec<Vec<u32>>,
    /// Raw contour polylines in font coordinates (Y-up).
    /// Used for text stroke rendering.
    pub contours: Vec<Vec<[f32; 2]>>,
}

impl GlyphCache {
    pub fn new() -> Self {
        Self {
            meshes: HashMap::new(),
        }
    }

    /// Get or build the glyph mesh for the given font and glyph ID.
    pub fn get(
        &mut self,
        font: &typst::text::Font,
        glyph_id: u16,
    ) -> &GlyphMesh {
        let key = (font_key(font), glyph_id);
        self.meshes.entry(key).or_insert_with(|| {
            build_glyph_mesh(font, glyph_id)
        })
    }
}

/// Stable key for a font (pointer to its backing data).
fn font_key(font: &typst::text::Font) -> usize {
    let info = font.info();
    typst::utils::hash128(info) as usize
}

fn build_glyph_mesh(font: &typst::text::Font, glyph_id: u16) -> GlyphMesh {
    let ttf = font.ttf();
    let mut builder = ContourBuilder::new();
    let id = ttf_parser::GlyphId(glyph_id);
    let _ = ttf.outline_glyph(id, &mut builder);
    let contours = builder.finish();
    let mut mesh = triangulate_contours(&contours);
    // Store raw contours for text stroke rendering.
    mesh.contours = contours
        .iter()
        .map(|c| c.iter().map(|p| [p.x, p.y]).collect())
        .collect();
    mesh
}

// ── Triangulation via earcutr ──────────────────────────────────────────

/// Triangulate a set of contours (with hole support) into a GlyphMesh.
///
/// Uses earcutr (Mapbox earcut algorithm) which handles holes natively.
/// Contours are classified by winding direction: the largest-area contour
/// determines the "outer" winding; opposite-wound contours are holes.
fn triangulate_contours(contours: &[Vec<egui::Pos2>]) -> GlyphMesh {
    if contours.is_empty() {
        return GlyphMesh { vertices: Vec::new(), triangles: Vec::new(), boundary_loops: Vec::new(), contours: Vec::new() };
    }

    // Classify contours by winding direction.
    let areas: Vec<(usize, f32)> = contours.iter()
        .enumerate()
        .filter(|(_, c)| c.len() >= 3)
        .map(|(i, c)| (i, signed_area_2(c)))
        .collect();

    if areas.is_empty() {
        return GlyphMesh { vertices: Vec::new(), triangles: Vec::new(), boundary_loops: Vec::new(), contours: Vec::new() };
    }

    // The largest absolute-area contour determines the "outer" winding.
    let outer_sign = areas.iter()
        .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
        .unwrap()
        .1;

    let mut outers: Vec<usize> = Vec::new();
    let mut holes: Vec<usize> = Vec::new();

    for &(idx, area) in &areas {
        if area * outer_sign > 0.0 {
            outers.push(idx);
        } else {
            holes.push(idx);
        }
    }

    // If all same winding, treat everything as outer (no holes).
    if outers.is_empty() {
        outers = holes.drain(..).collect();
    }

    let mut all_vertices: Vec<[f32; 2]> = Vec::new();
    let mut all_triangles: Vec<[u32; 3]> = Vec::new();
    let mut all_boundary_loops: Vec<Vec<u32>> = Vec::new();

    for &outer_idx in &outers {
        let outer = &contours[outer_idx];

        // Find holes that belong to this outer contour.
        let matched_holes: Vec<usize> = holes.iter()
            .filter(|&&h_idx| {
                let hole = &contours[h_idx];
                // Test if any vertex of the hole is inside the outer polygon.
                hole.iter().any(|p| point_in_polygon(*p, outer))
            })
            .copied()
            .collect();

        // Build flat coordinate array for earcutr:
        // [outer_x0, outer_y0, outer_x1, outer_y1, ..., hole0_x0, hole0_y0, ...]
        let mut coords: Vec<f64> = Vec::new();
        let mut hole_indices: Vec<usize> = Vec::new();
        // Track contour boundaries for feathering.
        let mut contour_ranges: Vec<(u32, u32)> = Vec::new(); // (start, count)

        let base = all_vertices.len() as u32;

        // Outer ring — earcutr expects CCW.
        let ring_start = (coords.len() / 2) as u32;
        let area = signed_area_2(outer);
        if area < 0.0 {
            for p in outer.iter().rev() {
                coords.push(p.x as f64);
                coords.push(p.y as f64);
            }
        } else {
            for p in outer {
                coords.push(p.x as f64);
                coords.push(p.y as f64);
            }
        }
        contour_ranges.push((ring_start, outer.len() as u32));

        // Hole rings — earcutr expects CW for holes.
        for &h_idx in &matched_holes {
            let hole = &contours[h_idx];
            hole_indices.push(coords.len() / 2);

            let ring_start = (coords.len() / 2) as u32;
            let area_h = signed_area_2(hole);
            if area_h > 0.0 {
                for p in hole.iter().rev() {
                    coords.push(p.x as f64);
                    coords.push(p.y as f64);
                }
            } else {
                for p in hole {
                    coords.push(p.x as f64);
                    coords.push(p.y as f64);
                }
            }
            contour_ranges.push((ring_start, hole.len() as u32));
        }

        // Triangulate.
        let tri_indices = earcutr::earcut(&coords, &hole_indices, 2)
            .unwrap_or_default();

        // Convert flat coords to vertices and triangle indices.
        let n_verts = coords.len() / 2;
        for i in 0..n_verts {
            all_vertices.push([coords[i * 2] as f32, coords[i * 2 + 1] as f32]);
        }

        for chunk in tri_indices.chunks_exact(3) {
            all_triangles.push([
                base + chunk[0] as u32,
                base + chunk[1] as u32,
                base + chunk[2] as u32,
            ]);
        }

        // Record boundary loops (vertex indices in contour order).
        for (start, count) in contour_ranges {
            let loop_indices: Vec<u32> = (0..count)
                .map(|i| base + start + i)
                .collect();
            all_boundary_loops.push(loop_indices);
        }
    }

    GlyphMesh {
        vertices: all_vertices,
        triangles: all_triangles,
        boundary_loops: all_boundary_loops,
        contours: Vec::new(), // filled by build_glyph_mesh after this call
    }
}

/// 2x signed area of a polygon (shoelace formula).
/// Positive = CCW in standard math coords (Y-up).
fn signed_area_2(pts: &[egui::Pos2]) -> f32 {
    let n = pts.len();
    let mut area = 0.0f32;
    for i in 0..n {
        let j = (i + 1) % n;
        area += pts[i].x * pts[j].y - pts[j].x * pts[i].y;
    }
    area
}

/// Ray-casting point-in-polygon test.
fn point_in_polygon(p: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    let n = polygon.len();
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[j];
        if (a.y > p.y) != (b.y > p.y) {
            let x_intersect = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if p.x < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

// ── Multi-subpath triangulation (for Typst Curve fill with FillRule) ──

/// Triangulate multiple closed screen-coordinate subpaths into a colored mesh.
///
/// Used for rendering Typst Curve geometry when there are multiple subpaths
/// (e.g., shapes with holes like rings or compound paths).
///
/// `even_odd`: if true, uses even-odd fill rule; otherwise non-zero winding.
pub fn triangulate_subpaths(
    subpaths: &[Vec<egui::Pos2>],
    even_odd: bool,
    color: egui::Color32,
) -> epaint::Mesh {
    let mut mesh = epaint::Mesh::default();

    let valid: Vec<&Vec<egui::Pos2>> = subpaths.iter().filter(|p| p.len() >= 3).collect();
    if valid.is_empty() {
        return mesh;
    }

    // Classify subpaths as outers/holes based on fill rule.
    let areas: Vec<f32> = valid.iter().map(|p| signed_area_2(p)).collect();
    let (outers, holes) = if even_odd {
        classify_even_odd(&valid)
    } else {
        classify_non_zero(&areas)
    };

    for &outer_idx in &outers {
        let outer = valid[outer_idx];

        let matched_holes: Vec<usize> = holes
            .iter()
            .filter(|&&h| valid[h].iter().any(|p| point_in_polygon(*p, outer)))
            .copied()
            .collect();

        let base = mesh.vertices.len() as u32;
        let mut coords: Vec<f64> = Vec::new();
        let mut hole_indices: Vec<usize> = Vec::new();

        for p in outer.iter() {
            coords.push(p.x as f64);
            coords.push(p.y as f64);
        }
        for &h_idx in &matched_holes {
            hole_indices.push(coords.len() / 2);
            for p in valid[h_idx].iter() {
                coords.push(p.x as f64);
                coords.push(p.y as f64);
            }
        }

        let tri = earcutr::earcut(&coords, &hole_indices, 2).unwrap_or_default();
        let n_verts = coords.len() / 2;
        for i in 0..n_verts {
            mesh.vertices.push(epaint::Vertex {
                pos: egui::pos2(coords[i * 2] as f32, coords[i * 2 + 1] as f32),
                uv: epaint::WHITE_UV,
                color,
            });
        }
        for chunk in tri.chunks_exact(3) {
            mesh.indices
                .push(base + chunk[0] as u32);
            mesh.indices
                .push(base + chunk[1] as u32);
            mesh.indices
                .push(base + chunk[2] as u32);
        }
    }

    mesh
}

/// Non-zero winding: largest-area contour determines "outer" winding.
fn classify_non_zero(areas: &[f32]) -> (Vec<usize>, Vec<usize>) {
    let outer_sign = areas
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.abs().partial_cmp(&b.abs()).unwrap())
        .map(|(_, a)| a.signum())
        .unwrap_or(1.0);

    let mut outers = Vec::new();
    let mut holes = Vec::new();
    for (i, &a) in areas.iter().enumerate() {
        if a * outer_sign >= 0.0 {
            outers.push(i);
        } else {
            holes.push(i);
        }
    }
    if outers.is_empty() {
        outers = holes.drain(..).collect();
    }
    (outers, holes)
}

/// Even-odd: classify by containment depth (even = outer, odd = hole).
fn classify_even_odd(paths: &[&Vec<egui::Pos2>]) -> (Vec<usize>, Vec<usize>) {
    let n = paths.len();
    let mut outers = Vec::new();
    let mut holes = Vec::new();
    for i in 0..n {
        let test_point = paths[i][0];
        let depth: usize = (0..n)
            .filter(|&j| i != j && point_in_polygon(test_point, paths[j]))
            .count();
        if depth % 2 == 0 {
            outers.push(i);
        } else {
            holes.push(i);
        }
    }
    (outers, holes)
}

// ── Rendering ──────────────────────────────────────────────────────────

/// Render a glyph mesh into an `epaint::Mesh` at the given screen position,
/// scale, and color, with anti-aliasing feathering.
///
/// `scale` = font_size_pt / units_per_em (converts font units → screen points).
/// `origin` = screen position of the glyph's baseline-left corner.
/// `feathering` = anti-aliasing width in screen points (typically 1.0 / pixels_per_point).
/// Font Y-up is flipped to screen Y-down.
pub fn render_glyph_mesh(
    glyph: &GlyphMesh,
    origin: egui::Pos2,
    scale: f32,
    color: egui::Color32,
    feathering: f32,
) -> epaint::Mesh {
    let mut mesh = epaint::Mesh::default();
    if glyph.vertices.is_empty() {
        return mesh;
    }

    let n_verts = glyph.vertices.len();
    let n_boundary: usize = glyph.boundary_loops.iter().map(|l| l.len()).sum();

    // Reserve: original verts + outer feathering verts, triangles + feathering quads.
    mesh.vertices.reserve(n_verts + n_boundary);
    mesh.indices.reserve(glyph.triangles.len() * 3 + n_boundary * 6);

    // Compute per-vertex outward normals (in screen coords) from boundary loops.
    // Interior vertices get zero normal (no shifting).
    let mut normals = vec![egui::Vec2::ZERO; n_verts];

    for loop_indices in &glyph.boundary_loops {
        let n = loop_indices.len();
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let vi = loop_indices[i] as usize;
            let prev = loop_indices[(i + n - 1) % n] as usize;
            let next = loop_indices[(i + 1) % n] as usize;

            // Edge vectors in screen coords (Y-flipped).
            let [px, py] = glyph.vertices[prev];
            let [cx, cy] = glyph.vertices[vi];
            let [nx, ny] = glyph.vertices[next];

            let e0 = egui::vec2((cx - px) * scale, -(cy - py) * scale);
            let e1 = egui::vec2((nx - cx) * scale, -(ny - cy) * scale);

            // Perpendicular normals (outward = left-hand side of screen-space edge).
            // Font contours are CCW (Y-up) → CW after Y-flip to screen (Y-down).
            // Left-hand perpendicular of a CW winding points outward.
            // For holes (CW font → CCW screen), left-hand perp points into the hole,
            // which is also "away from filled area" — correct for both cases.
            let n0 = egui::vec2(-e0.y, e0.x);
            let n1 = egui::vec2(-e1.y, e1.x);

            // Average and normalize for miter join.
            let avg = n0 + n1;
            let len = avg.length();
            normals[vi] = if len > 1e-6 { avg / len } else { egui::Vec2::ZERO };
        }
    }

    // Emit inner vertices (shifted inward by feathering/2).
    let half_f = feathering * 0.5;
    for (i, &[x, y]) in glyph.vertices.iter().enumerate() {
        let screen_pos = egui::pos2(origin.x + x * scale, origin.y - y * scale);
        let shifted = screen_pos - normals[i] * half_f;
        mesh.vertices.push(epaint::Vertex {
            pos: shifted,
            uv: epaint::WHITE_UV,
            color,
        });
    }

    // Fill triangles (using inner vertices).
    for &[a, b, c] in &glyph.triangles {
        mesh.indices.push(a);
        mesh.indices.push(b);
        mesh.indices.push(c);
    }

    // Emit outer feathering vertices and quads.
    let outer_base = n_verts as u32;
    let mut outer_offset = 0u32;

    for loop_indices in &glyph.boundary_loops {
        let n = loop_indices.len();
        if n < 3 {
            continue;
        }

        // Emit outer vertices for this loop.
        for &vi in loop_indices {
            let [x, y] = glyph.vertices[vi as usize];
            let screen_pos = egui::pos2(origin.x + x * scale, origin.y - y * scale);
            let shifted = screen_pos + normals[vi as usize] * half_f;
            mesh.vertices.push(epaint::Vertex {
                pos: shifted,
                uv: epaint::WHITE_UV,
                color: egui::Color32::TRANSPARENT,
            });
        }

        // Feathering quads: connect inner edge to outer edge.
        for i in 0..n {
            let i0 = i;
            let i1 = (i + 1) % n;

            let inner0 = loop_indices[i0];
            let inner1 = loop_indices[i1];
            let outer0 = outer_base + outer_offset + i0 as u32;
            let outer1 = outer_base + outer_offset + i1 as u32;

            // Two triangles per edge segment.
            mesh.indices.push(inner0);
            mesh.indices.push(inner1);
            mesh.indices.push(outer0);

            mesh.indices.push(inner1);
            mesh.indices.push(outer1);
            mesh.indices.push(outer0);
        }

        outer_offset += n as u32;
    }

    mesh
}

// ── Contour extraction ─────────────────────────────────────────────────

/// Tolerance for flattening bezier curves to polylines (in font design units).
const FLATTEN_TOLERANCE: f32 = 1.0;

/// Collects glyph outline commands into polyline contours.
struct ContourBuilder {
    contours: Vec<Vec<egui::Pos2>>,
    current: Vec<egui::Pos2>,
}

impl ContourBuilder {
    fn new() -> Self {
        Self {
            contours: Vec::new(),
            current: Vec::new(),
        }
    }

    fn finish(mut self) -> Vec<Vec<egui::Pos2>> {
        if !self.current.is_empty() {
            self.contours.push(self.current);
        }
        self.contours
    }

    fn last_point(&self) -> egui::Pos2 {
        self.current
            .last()
            .copied()
            .unwrap_or(egui::pos2(0.0, 0.0))
    }
}

impl ttf_parser::OutlineBuilder for ContourBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
        self.current.push(egui::pos2(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push(egui::pos2(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p0 = self.last_point();
        let p1 = egui::pos2(x1, y1);
        let p2 = egui::pos2(x, y);
        flatten_quad(p0, p1, p2, FLATTEN_TOLERANCE, &mut self.current);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p0 = self.last_point();
        let p1 = egui::pos2(x1, y1);
        let p2 = egui::pos2(x2, y2);
        let p3 = egui::pos2(x, y);
        flatten_cubic(p0, p1, p2, p3, FLATTEN_TOLERANCE, &mut self.current);
    }

    fn close(&mut self) {
        if !self.current.is_empty() {
            self.contours.push(std::mem::take(&mut self.current));
        }
    }
}

// ── Bezier flattening ──────────────────────────────────────────────────

fn flatten_quad(
    p0: egui::Pos2,
    p1: egui::Pos2,
    p2: egui::Pos2,
    tolerance: f32,
    out: &mut Vec<egui::Pos2>,
) {
    let mid = egui::pos2((p0.x + p2.x) * 0.5, (p0.y + p2.y) * 0.5);
    let d = ((p1.x - mid.x).powi(2) + (p1.y - mid.y).powi(2)).sqrt();
    if d <= tolerance {
        out.push(p2);
    } else {
        let q0 = egui::pos2((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        let q1 = egui::pos2((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
        let r0 = egui::pos2((q0.x + q1.x) * 0.5, (q0.y + q1.y) * 0.5);
        flatten_quad(p0, q0, r0, tolerance, out);
        flatten_quad(r0, q1, p2, tolerance, out);
    }
}

fn flatten_cubic(
    p0: egui::Pos2,
    p1: egui::Pos2,
    p2: egui::Pos2,
    p3: egui::Pos2,
    tolerance: f32,
    out: &mut Vec<egui::Pos2>,
) {
    let d1 = point_line_distance(p1, p0, p3);
    let d2 = point_line_distance(p2, p0, p3);
    if d1 <= tolerance && d2 <= tolerance {
        out.push(p3);
    } else {
        let q0 = mid(p0, p1);
        let q1 = mid(p1, p2);
        let q2 = mid(p2, p3);
        let r0 = mid(q0, q1);
        let r1 = mid(q1, q2);
        let s0 = mid(r0, r1);
        flatten_cubic(p0, q0, r0, s0, tolerance, out);
        flatten_cubic(s0, r1, q2, p3, tolerance, out);
    }
}

fn mid(a: egui::Pos2, b: egui::Pos2) -> egui::Pos2 {
    egui::pos2((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn point_line_distance(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-10 {
        ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt()
    } else {
        ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len_sq.sqrt()
    }
}
