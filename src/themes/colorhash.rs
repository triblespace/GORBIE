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

    /// Create a hasher initialized with the FNV-1a offset basis.
    pub fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    /// Feed a byte slice into the running hash.
    pub fn update(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(Self::PRIME);
        }
        self.0 = hash;
    }

    /// Feed a `u64` (little-endian) into the running hash.
    pub fn update_u64(&mut self, value: u64) {
        self.update(&value.to_le_bytes());
    }

    /// Consume the hasher and return the final 64-bit digest.
    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot FNV-1a hash of a byte slice, returning a 64-bit digest.
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.update(bytes);
    h.finish()
}

/// Map a hash value to an index in a palette of the given length.
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
    1028, // melon yellow
    2010, // signal orange
    2004, // pure orange
    3014, // antique pink
    3020, // traffic red
    3004, // purple red
    4008, // signal violet
    4005, // blue lilac
    5005, // signal blue
    5015, // sky blue
    5021, // water blue
    5024, // pastel blue
    6018, // yellow green
    6027, // light green
    6032, // signal green
    6033, // mint turquoise
    6017, // may green
    1012, // lemon yellow
    2012, // salmon orange
    3015, // light pink
    4009, // pastel violet
    5012, // light blue
    6019, // pastel green
];

/// Pick a color from [`RAL_CATEGORICAL`] using a pre-computed hash.
pub fn ral_categorical_from_hash(hash: u64) -> Color32 {
    ral_from_hash_in_palette(hash, RAL_CATEGORICAL)
}

/// Pick a RAL color from an arbitrary palette slice using a pre-computed hash.
pub fn ral_from_hash_in_palette(hash: u64, palette: &[u16]) -> Color32 {
    if palette.is_empty() {
        return themes::ral(9011);
    }
    let idx = palette_index(hash, palette.len());
    themes::ral(palette[idx])
}

/// Hash a byte slice and return a categorical RAL color.
pub fn ral_categorical(bytes: &[u8]) -> Color32 {
    ral_categorical_from_hash(hash64(bytes))
}

/// Hash a two-part string key (null-separated) and return a categorical RAL color.
pub fn ral_categorical_key(a: &str, b: &str) -> Color32 {
    let mut h = Fnv1a64::new();
    h.update(a.as_bytes());
    h.update(&[0]);
    h.update(b.as_bytes());
    ral_categorical_from_hash(h.finish())
}

/// Approximate perceptual luminance of a color in sRGB space (0.0 = black, 1.0 = white).
pub fn luma(color: Color32) -> f32 {
    // Cheap, perceptual-ish luma in sRGB space.
    let r = color.r() as f32 / 255.0;
    let g = color.g() as f32 / 255.0;
    let b = color.b() as f32 / 255.0;
    0.299 * r + 0.587 * g + 0.114 * b
}

/// Return black or white text depending on the background's luminance.
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
