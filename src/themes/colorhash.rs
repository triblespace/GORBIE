use egui::Color32;
use egui::Stroke;

use crate::themes;

/// A small, deterministic hash for turning "content" into a stable palette index.
///
/// This is intentionally not cryptographic; it's for UI color bucketing.
#[derive(Clone, Copy, Debug)]
pub struct Fnv1a64(u64);

impl Fnv1a64 {
    const OFFSET_BASIS: u64 = 1469598103934665603;
    const PRIME: u64 = 1099511628211;

    pub fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    pub fn update(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(Self::PRIME);
        }
        self.0 = hash;
    }

    pub fn update_u64(&mut self, value: u64) {
        self.update(&value.to_le_bytes());
    }

    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.update(bytes);
    h.finish()
}

pub fn palette_index(hash: u64, palette_len: usize) -> usize {
    if palette_len == 0 {
        0
    } else {
        (hash as usize) % palette_len
    }
}

/// A small, visually distinct RAL palette meant for categorical coloring.
///
/// Note: Avoids `RAL 2009` (the UI accent) so selection outlines stay legible.
pub const RAL_CATEGORICAL: &[u16] = &[
    1003, // signal yellow
    2010, // signal orange
    3014, // antique pink
    3020, // traffic red
    4008, // signal violet
    5005, // signal blue
    5015, // sky blue
    5021, // water blue
    6018, // yellow green
    6027, // light green
    6032, // signal green
    6033, // mint turquoise
];

pub fn ral_categorical_from_hash(hash: u64) -> Color32 {
    ral_from_hash_in_palette(hash, RAL_CATEGORICAL)
}

pub fn ral_from_hash_in_palette(hash: u64, palette: &[u16]) -> Color32 {
    if palette.is_empty() {
        return themes::ral(9011);
    }
    let idx = palette_index(hash, palette.len());
    themes::ral(palette[idx])
}

pub fn ral_categorical(bytes: &[u8]) -> Color32 {
    ral_categorical_from_hash(hash64(bytes))
}

pub fn ral_categorical_key(a: &str, b: &str) -> Color32 {
    let mut h = Fnv1a64::new();
    h.update(a.as_bytes());
    h.update(&[0]);
    h.update(b.as_bytes());
    ral_categorical_from_hash(h.finish())
}

pub fn luma(color: Color32) -> f32 {
    // Cheap, perceptual-ish luma in sRGB space.
    let r = color.r() as f32 / 255.0;
    let g = color.g() as f32 / 255.0;
    let b = color.b() as f32 / 255.0;
    0.299 * r + 0.587 * g + 0.114 * b
}

pub fn text_color_on(background: Color32) -> Color32 {
    if luma(background) > 0.55 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// A thick, high-contrast outline stroke for emphasizing a colored region.
pub fn highlight_stroke(fill: Color32) -> Stroke {
    Stroke::new(2.0, text_color_on(fill))
}
