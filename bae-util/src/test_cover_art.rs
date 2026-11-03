//! Synthetic cover images (test-fixtures/cover-art, described in its README),
//! for the tests of this crate and every crate that stores or serves covers.

/// 16 × 8, one solid color, 51 bytes.
pub static SOLID_GIF: &[u8] = include_bytes!("../test-fixtures/cover-art/solid.gif");
/// 16 × 8, one solid color, 38 bytes.
pub static SOLID_WEBP: &[u8] = include_bytes!("../test-fixtures/cover-art/solid.webp");
/// Two 1200 × 600 frames, 100 ms each.
pub static ANIMATED_GIF: &[u8] = include_bytes!("../test-fixtures/cover-art/animated.gif");
/// Two 1200 × 600 frames, 100 ms each.
pub static ANIMATED_WEBP: &[u8] = include_bytes!("../test-fixtures/cover-art/animated.webp");
/// 16 × 8, opaque in its left half and transparent in its right.
pub static TRANSPARENT_GIF: &[u8] = include_bytes!("../test-fixtures/cover-art/transparent.gif");
/// 16 × 8, half-transparent throughout.
pub static TRANSPARENT_WEBP: &[u8] = include_bytes!("../test-fixtures/cover-art/transparent.webp");
