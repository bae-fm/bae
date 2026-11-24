//! What a picture is read from.

use std::io::{Read, Seek};

/// What a picture is read from: an open file, or the bytes of art an audio
/// file carries.
pub trait PictureOrigin: Read + Seek + Send + 'static {}

impl<T: Read + Seek + Send + 'static> PictureOrigin for T {}
