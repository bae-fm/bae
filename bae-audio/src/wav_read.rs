//! A reader for the WAV streams bae serves, for tests that check one byte by
//! byte: the RIFF header's sizes, the `fmt ` chunk's PCM description, and the
//! `data` payload.

/// One WAV stream, read: its declared sizes, its PCM shape, and its samples.
#[derive(Debug)]
pub struct StreamedWav {
    /// The RIFF chunk's declared size; `u32::MAX` says "unknown".
    pub riff_size: u32,
    /// The `data` chunk's declared size; `u32::MAX` says "unknown".
    pub data_size: u32,
    pub channels: u32,
    pub sample_rate: u32,
    pub bits_per_sample: u32,
    /// The `data` payload: every byte after its header when the size is
    /// unknown, else exactly the declared size.
    pub data: Vec<u8>,
}

/// `WAVE_FORMAT_PCM`.
const FORMAT_PCM: u16 = 1;
/// `WAVE_FORMAT_EXTENSIBLE`, whose sub-format GUID then names PCM.
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;
/// `KSDATAFORMAT_SUBTYPE_PCM`'s first two bytes; the rest is the fixed base GUID.
const SUBFORMAT_PCM: u16 = 1;

/// Read `bytes` as a PCM WAV stream. The error says which part is not one.
pub fn parse_streamed_wav(bytes: &[u8]) -> Result<StreamedWav, String> {
    let u16_at = |at: usize| -> Result<u16, String> {
        bytes
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| format!("stream ends inside a 16-bit field at byte {at}"))
    };
    let u32_at = |at: usize| -> Result<u32, String> {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| format!("stream ends inside a 32-bit field at byte {at}"))
    };
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a RIFF/WAVE stream".to_string());
    }
    let riff_size = u32_at(4)?;

    let mut format = None;
    let mut at = 12;
    loop {
        let id = bytes
            .get(at..at + 4)
            .ok_or("stream ends before a data chunk")?;
        let size = u32_at(at + 4)?;
        let body = at + 8;
        match id {
            b"fmt " => {
                let tag = u16_at(body)?;
                let pcm = match tag {
                    FORMAT_PCM => true,
                    FORMAT_EXTENSIBLE => u16_at(body + 24)? == SUBFORMAT_PCM,
                    _ => false,
                };
                if !pcm {
                    return Err(format!("fmt chunk is not PCM (format tag {tag:#x})"));
                }
                format = Some((
                    u32::from(u16_at(body + 2)?),
                    u32_at(body + 4)?,
                    u32::from(u16_at(body + 14)?),
                ));
            }
            b"data" => {
                let (channels, sample_rate, bits_per_sample) =
                    format.ok_or("data chunk before the fmt chunk")?;
                let data = if size == u32::MAX {
                    bytes[body..].to_vec()
                } else {
                    bytes
                        .get(body..body + size as usize)
                        .ok_or("stream ends before its declared data size")?
                        .to_vec()
                };
                return Ok(StreamedWav {
                    riff_size,
                    data_size: size,
                    channels,
                    sample_rate,
                    bits_per_sample,
                    data,
                });
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        at = body + size as usize + (size as usize & 1);
    }
}
