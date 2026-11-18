//! What a cast asks the person who cast for, when an AirPlay receiver wants
//! a PIN or a password, and the lines the app's sheet shows for each request.

/// The code an AirPlay receiver asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAirPlayCode {
    /// The PIN the receiver shows, `digits` digits long.
    Pin { digits: u8 },
    /// The password set on the receiver.
    Password,
}

mirror_enum! {
    BridgeAirPlayCode = bae_cast::CodeKind,
    from_core: fn,
    variants: { Pin { digits }, Password },
}

/// Why bae asks for the code now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAirPlayCodeAsked {
    /// bae has no pairing with the receiver.
    FirstPairing,
    /// The receiver asks for a PIN on every cast.
    EveryCast,
    /// The receiver refused the pairing bae kept for it.
    PairingLost,
}

mirror_enum! {
    BridgeAirPlayCodeAsked = bae_cast::CodeAsked,
    from_core: fn,
    variants: { FirstPairing, EveryCast, PairingLost },
}

/// What was wrong with the last code the person entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAirPlayCodeRefused {
    /// The receiver didn't take it; a receiver that shows a PIN shows a new
    /// one.
    WrongCode,
    /// It didn't have the code's shape, so bae sent it nowhere.
    WrongShape,
}

mirror_enum! {
    BridgeAirPlayCodeRefused = bae_cast::CodeRefused,
    from_core: fn,
    variants: { WrongCode, WrongShape },
}

/// What an AirPlay receiver asks the person for, and why bae asks now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAirPlayCodeRequest {
    pub code: BridgeAirPlayCode,
    pub why: BridgeAirPlayCodeAsked,
    pub last_try: Option<BridgeAirPlayCodeRefused>,
}

mirror_struct! {
    BridgeAirPlayCodeRequest = bae_cast::CodeRequest,
    from_core: pub(crate) fn,
    fields: {
        code: (BridgeAirPlayCode),
        why: (BridgeAirPlayCodeAsked),
        last_try: (opt BridgeAirPlayCodeRefused),
    },
}

/// The catalog key of the line under the sheet's title for `request`: what
/// to enter, and why bae asks. The line takes the receiver's name as
/// `receiver`.
#[uniffi::export]
pub fn bridge_airplay_code_message_key(request: BridgeAirPlayCodeRequest) -> String {
    use BridgeAirPlayCodeAsked as Asked;
    let wrong = request.last_try == Some(BridgeAirPlayCodeRefused::WrongCode);
    match (request.code, request.why) {
        // A wrong PIN makes the receiver show a new one.
        (BridgeAirPlayCode::Pin { .. }, _) if wrong => "core.airplay.code.message.pin_after_wrong",
        (BridgeAirPlayCode::Pin { .. }, Asked::FirstPairing) => {
            "core.airplay.code.message.pin_first"
        }
        (BridgeAirPlayCode::Pin { .. }, Asked::EveryCast) => {
            "core.airplay.code.message.pin_every_cast"
        }
        (BridgeAirPlayCode::Pin { .. }, Asked::PairingLost) => {
            "core.airplay.code.message.pin_pairing_lost"
        }
        (BridgeAirPlayCode::Password, Asked::PairingLost) if !wrong => {
            "core.airplay.code.message.password_pairing_lost"
        }
        (BridgeAirPlayCode::Password, _) => "core.airplay.code.message.password",
    }
    .to_string()
}

/// The catalog key of the red line under the sheet's field for `request`, or
/// `None` when the last code had nothing wrong. A PIN's shape line takes its
/// `digits`.
#[uniffi::export]
pub fn bridge_airplay_code_field_key(request: BridgeAirPlayCodeRequest) -> Option<String> {
    let key = match (request.last_try?, request.code) {
        (BridgeAirPlayCodeRefused::WrongCode, BridgeAirPlayCode::Pin { .. }) => {
            "core.airplay.code.field.pin_wrong"
        }
        (BridgeAirPlayCodeRefused::WrongCode, BridgeAirPlayCode::Password) => {
            "core.airplay.code.field.password_wrong"
        }
        (BridgeAirPlayCodeRefused::WrongShape, BridgeAirPlayCode::Pin { .. }) => {
            "core.airplay.code.field.pin_shape"
        }
        (BridgeAirPlayCodeRefused::WrongShape, BridgeAirPlayCode::Password) => {
            "core.airplay.code.field.password_shape"
        }
    };
    Some(key.to_string())
}

#[cfg(test)]
#[path = "airplay_code_tests.rs"]
mod tests;
