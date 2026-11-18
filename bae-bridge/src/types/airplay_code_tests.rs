use super::*;

fn request(
    code: BridgeAirPlayCode,
    why: BridgeAirPlayCodeAsked,
    last_try: Option<BridgeAirPlayCodeRefused>,
) -> BridgeAirPlayCodeRequest {
    BridgeAirPlayCodeRequest { code, why, last_try }
}

/// The line under the title says why bae asks; after a wrong PIN it asks for
/// the new PIN the receiver shows, whatever the reason, and a wrong
/// password asks for the password again without saying the receiver forgot
/// this device.
#[test]
fn the_message_says_why_bae_asks_and_what_to_enter() {
    use BridgeAirPlayCodeAsked as Asked;
    use BridgeAirPlayCodeRefused as Refused;
    let pin = BridgeAirPlayCode::Pin { digits: 4 };
    let password = BridgeAirPlayCode::Password;
    for (request, key) in [
        (request(pin, Asked::FirstPairing, None), "pin_first"),
        (request(pin, Asked::EveryCast, None), "pin_every_cast"),
        (request(pin, Asked::PairingLost, None), "pin_pairing_lost"),
        (request(pin, Asked::PairingLost, Some(Refused::WrongCode)), "pin_after_wrong"),
        (request(pin, Asked::EveryCast, Some(Refused::WrongShape)), "pin_every_cast"),
        (request(password, Asked::FirstPairing, None), "password"),
        (request(password, Asked::PairingLost, None), "password_pairing_lost"),
        (request(password, Asked::PairingLost, Some(Refused::WrongCode)), "password"),
    ] {
        assert_eq!(
            bridge_airplay_code_message_key(request),
            format!("core.airplay.code.message.{key}"),
            "{request:?}"
        );
    }
}

/// The line under the field says what was wrong with the last code, and
/// nothing before the first.
#[test]
fn the_field_line_says_what_was_wrong_with_the_last_code() {
    use BridgeAirPlayCodeAsked as Asked;
    use BridgeAirPlayCodeRefused as Refused;
    let pin = BridgeAirPlayCode::Pin { digits: 4 };
    let password = BridgeAirPlayCode::Password;
    assert_eq!(bridge_airplay_code_field_key(request(pin, Asked::FirstPairing, None)), None);
    for (request, key) in [
        (request(pin, Asked::FirstPairing, Some(Refused::WrongCode)), "pin_wrong"),
        (request(pin, Asked::FirstPairing, Some(Refused::WrongShape)), "pin_shape"),
        (request(password, Asked::FirstPairing, Some(Refused::WrongCode)), "password_wrong"),
        (request(password, Asked::FirstPairing, Some(Refused::WrongShape)), "password_shape"),
    ] {
        assert_eq!(
            bridge_airplay_code_field_key(request),
            Some(format!("core.airplay.code.field.{key}")),
            "{request:?}"
        );
    }
}
