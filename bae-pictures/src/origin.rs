//! What a picture is read from.

use std::io::{Read, Seek, SeekFrom};

/// What a picture is read from: an open file, or the bytes of art an audio
/// file carries.
pub trait PictureOrigin: Read + Seek + Send + 'static {}

impl<T: Read + Seek + Send + 'static> PictureOrigin for T {}

/// A reader of a picture's original that holds `held` for as long as it is
/// open: the place of a budget of open files, or of a volume's reads.
pub struct HeldOrigin<Reader, Held> {
    reader: Reader,
    _held: Held,
}

impl<Reader, Held> HeldOrigin<Reader, Held> {
    pub fn new(reader: Reader, held: Held) -> Self {
        Self { reader, _held: held }
    }
}

impl<Reader: Read, Held> Read for HeldOrigin<Reader, Held> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl<Reader: Seek, Held> Seek for HeldOrigin<Reader, Held> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(position)
    }
}
