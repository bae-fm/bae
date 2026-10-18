//! The restore code a person's own device shows to add their new device, and
//! what a scan of it reads back.
//!
//! On S3 the QR code also carries the key this device holds, beside the code
//! and never inside it (coven §13.1): the person controls both devices, so the
//! new one restores without asking for the key. The typed code carries no key.

use base64::Engine as _;

/// Separates the restore code from the S3 key in a QR code's text. Neither a
/// code (`CVR1-` and uppercase base32) nor URL-safe base64 contains it.
const KEY_SEPARATOR: char = '.';

/// What this device shows for its person's restore code: the code to type or
/// copy, and the QR text, which adds this device's S3 key when it holds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreCodeShare {
    pub code: String,
    pub qr_text: String,
    /// Whether the QR text carries the S3 key, so scanning it asks for none.
    pub qr_carries_s3_key: bool,
}

impl RestoreCodeShare {
    /// Show `code`, with `s3_key` beside it in the QR text.
    pub fn new(
        code: String,
        s3_key: Option<&coven::S3Credentials>,
    ) -> Result<Self, coven::StorageError> {
        let qr_text = match s3_key {
            Some(key) => format!(
                "{code}{KEY_SEPARATOR}{}",
                base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(key.encode()?.as_bytes())
            ),
            None => code.clone(),
        };
        Ok(Self {
            code,
            qr_text,
            qr_carries_s3_key: s3_key.is_some(),
        })
    }
}

/// A scanned QR code or pasted text: the code, and the S3 key that came
/// beside it from the person's own device.
pub struct ScannedCode {
    pub code: String,
    pub s3_credentials: Option<coven::S3Credentials>,
}

/// The text beside a scanned code isn't an S3 key.
#[derive(Debug, thiserror::Error)]
pub enum ScannedCodeError {
    #[error("the key beside the code is not base64: {0}")]
    Encoding(#[from] base64::DecodeError),
    #[error("the key beside the code is not an S3 key: {0}")]
    Key(#[from] coven::StorageError),
}

/// Read a scanned or pasted restore QR's text. Text with no key beside it —
/// an invite, or a code typed by hand — is the code alone; coven decides
/// whether the code itself is valid.
pub fn read_scanned_code(text: &str) -> Result<ScannedCode, ScannedCodeError> {
    let text = text.trim();
    let Some((code, key)) = text.split_once(KEY_SEPARATOR) else {
        return Ok(ScannedCode {
            code: text.to_string(),
            s3_credentials: None,
        });
    };
    let bytes =
        coven::SecretBytes::new(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(key)?);
    Ok(ScannedCode {
        code: code.to_string(),
        s3_credentials: Some(coven::S3Credentials::decode(bytes.as_bytes())?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> coven::S3Credentials {
        coven::S3Credentials {
            access_key_id: "access-id".into(),
            secret_access_key: coven::SecretText::new("secret".into()),
        }
    }

    #[test]
    fn a_shown_key_scans_back_beside_its_code() {
        let share = RestoreCodeShare::new("CVR1-ABC234".into(), Some(&key())).unwrap();
        assert_eq!(share.code, "CVR1-ABC234");
        assert!(share.qr_carries_s3_key);
        let scanned = read_scanned_code(&share.qr_text).unwrap();
        assert_eq!(scanned.code, "CVR1-ABC234");
        let credentials = scanned.s3_credentials.unwrap();
        assert_eq!(credentials.access_key_id, "access-id");
        assert_eq!(credentials.secret_access_key.as_str(), "secret");
    }

    #[test]
    fn a_code_without_a_key_scans_as_the_code_alone() {
        let share = RestoreCodeShare::new("CVR1-ABC234".into(), None).unwrap();
        assert_eq!(share.qr_text, "CVR1-ABC234");
        assert!(!share.qr_carries_s3_key);
        let scanned = read_scanned_code(" CVR1-ABC234\n").unwrap();
        assert_eq!(scanned.code, "CVR1-ABC234");
        assert!(scanned.s3_credentials.is_none());
    }

    #[test]
    fn text_beside_the_code_that_is_no_key_is_refused() {
        assert!(matches!(
            read_scanned_code("CVR1-ABC234.not base64"),
            Err(ScannedCodeError::Encoding(_))
        ));
        let not_a_key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"{}");
        assert!(matches!(
            read_scanned_code(&format!("CVR1-ABC234.{not_a_key}")),
            Err(ScannedCodeError::Key(_))
        ));
    }
}
