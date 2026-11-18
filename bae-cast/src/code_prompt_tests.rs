use super::*;

/// A PIN fits only with all its digits, leading zeros kept; a password fits
/// with any character.
#[test]
fn a_code_fits_its_kind() {
    let pin = CodeKind::Pin { digits: 4 };
    assert!(pin.fits("0482"));
    for wrong in ["", "482", "04821", "04a2", "０４８２"] {
        assert!(!pin.fits(wrong), "{wrong:?}");
    }
    assert!(CodeKind::Password.fits(" "));
    assert!(!CodeKind::Password.fits(""));
}
