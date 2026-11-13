//! What Add Device's sheet shows of the pairing it started.

/// A pairing's id and the codes Add Device's sheet shows for it.
#[derive(Debug)]
pub struct AddingDevice {
    pub id: coven::AddDeviceId,
    /// The code to type or copy, in groups of four.
    pub code: String,
    /// The code for the QR code. On S3 it carries the key this device
    /// connects with, which the typed code does not.
    pub qr_text: String,
}
