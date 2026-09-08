//! Resident PNG delivery from the native headless renderer.
//!
//! This is the same card layout, settling, tiling and PNG encoding used by
//! file captures. It does not open a window or create an output directory.

use std::time::Duration;

/// A renderer or consumer failure. Accepted captures are never retried.
pub type CaptureResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Native headless rendering settings. Card width remains the notebook's
/// standard column width; tall cards are emitted as successive vertical tiles.
#[derive(Clone, Copy, Debug)]
pub struct CaptureOptions {
    pub pixels_per_point: f32,
    pub settle_timeout: Duration,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            pixels_per_point: crate::HEADLESS_DEFAULT_PIXELS_PER_POINT,
            settle_timeout: crate::HEADLESS_DEFAULT_SETTLE_TIMEOUT,
        }
    }
}

impl CaptureOptions {
    /// Validate before acquiring a GPU or evaluating the notebook body.
    /// A zero timeout intentionally captures the first frame immediately.
    pub fn validate(&self) -> CaptureResult<()> {
        let width = crate::NOTEBOOK_COLUMN_WIDTH * self.pixels_per_point;
        if !self.pixels_per_point.is_finite()
            || self.pixels_per_point <= 0.0
            || width.ceil() > (u32::MAX / 8) as f32
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "capture scale must be finite, positive and representable",
            )
            .into());
        }
        Ok(())
    }
}

/// One complete resident PNG, in notebook card order then vertical tile order.
/// Indices are zero-based; `tile_count` is positive. The encoded dimensions and
/// density metadata are identical to the corresponding CLI file capture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedPng {
    pub card_index: usize,
    pub tile_index: usize,
    pub tile_count: usize,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

impl CapturedPng {
    /// The existing file-capture name, without an output directory.
    pub fn filename(&self) -> String {
        if self.tile_count <= 1 {
            format!("card_{:04}.png", self.card_index + 1)
        } else {
            format!(
                "card_{:04}_p{:02}.png",
                self.card_index + 1,
                self.tile_index + 1
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_preserve_single_card_and_tiled_conventions() {
        let mut capture = CapturedPng {
            card_index: 0,
            tile_index: 0,
            tile_count: 1,
            width: 1,
            height: 1,
            bytes: Vec::new(),
        };
        assert_eq!(capture.filename(), "card_0001.png");
        capture.card_index = 9;
        capture.tile_count = 3;
        capture.tile_index = 1;
        assert_eq!(capture.filename(), "card_0010_p02.png");
    }

    #[test]
    fn invalid_scale_fails_before_notebook_body_or_gpu_acquisition() {
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            let result = crate::NotebookConfig::new("invalid capture").capture(
                CaptureOptions {
                    pixels_per_point: scale,
                    ..Default::default()
                },
                |_| panic!("invalid options must not evaluate notebook"),
                |_| panic!("invalid options must not emit"),
            );
            assert!(result.is_err(), "scale {scale}");
        }
        assert!(CaptureOptions {
            settle_timeout: Duration::ZERO,
            ..Default::default()
        }
        .validate()
        .is_ok());
    }
}
