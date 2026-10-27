//! The restore code a person's own device shows to add their new device.
//!
//! On S3 the QR code's code holds the key this device holds, inside the code
//! (coven §13.1): the person controls both devices, so the new one restores
//! without asking for the key. The code to type or copy holds no key.

/// What this device shows for its person's restore code: the code to type or
/// copy, and the code for the QR, which holds this device's S3 key when it
/// has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCodeShare {
    pub code: String,
    pub qr_text: String,
    /// Whether the QR's code holds the S3 key, so scanning it asks for none.
    pub qr_carries_s3_key: bool,
}

impl RestoreCodeShare {
    /// Show `code` to type, and `qr_text`, the code coven made for the QR.
    pub fn new(code: String, qr_text: String) -> Result<Self, coven::CodeError> {
        let qr_carries_s3_key = coven::decode_code_info(&qr_text)?.metadata().carries_s3_key;
        Ok(Self {
            code,
            qr_text,
            qr_carries_s3_key,
        })
    }
}
