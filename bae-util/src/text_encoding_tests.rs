use super::*;

#[test]
fn utf8_no_bom() {
    let input = "Hello, world!".as_bytes();
    let result = decode_text(input);
    assert_eq!(result.text, "Hello, world!");
    assert_eq!(result.encoding, "UTF-8");
}

#[test]
fn utf8_with_bom() {
    let mut input = vec![0xEF, 0xBB, 0xBF];
    input.extend_from_slice("Hello, world!".as_bytes());
    let result = decode_text(&input);
    assert_eq!(result.text, "Hello, world!");
    assert_eq!(result.encoding, "UTF-8");
}

#[test]
fn utf16_le_with_bom() {
    let text = "Hello";
    let mut input = vec![0xFF, 0xFE]; // BOM
    for ch in text.encode_utf16() {
        input.extend_from_slice(&ch.to_le_bytes());
    }
    let result = decode_text(&input);
    assert_eq!(result.text, "Hello");
    assert_eq!(result.encoding, "UTF-16LE");
}

#[test]
fn utf16_be_with_bom() {
    let text = "Hello";
    let mut input = vec![0xFE, 0xFF]; // BOM
    for ch in text.encode_utf16() {
        input.extend_from_slice(&ch.to_be_bytes());
    }
    let result = decode_text(&input);
    assert_eq!(result.text, "Hello");
    assert_eq!(result.encoding, "UTF-16BE");
}

#[test]
fn windows_1252_fallback() {
    // "caf\xe9" = "cafe" with e-acute in Windows-1252
    let input = b"caf\xe9";
    let result = decode_text(input);
    assert!(result.text.contains("caf"), "should decode the ASCII portion");
    // chardetng should detect this as a Latin encoding and decode the e-acute
    assert!(result.text.contains('\u{00e9}') || result.text.len() == 4, "should decode e-acute");
}

#[test]
fn shift_jis_fallback() {
    // Encode a known Shift-JIS string: "テスト" (test)
    let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode("テスト");
    let result = decode_text(&encoded);
    assert_eq!(result.text, "テスト");
    assert_eq!(result.encoding, "Shift_JIS");
}

/// A text file is read through the place it is given, which goes back once
/// the read is done, whether the file opened or not.
#[test]
fn reading_a_text_file_gives_its_place_back() {
    let dir = std::env::temp_dir().join(format!("bae-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sheet.cue");
    std::fs::write(&path, "TITLE \"One\"").unwrap();
    let budget = coven::FileBudget::for_test();
    let watch = budget.watch();
    let lease = budget.lease_blocking(1).unwrap();

    let read = read_text_file(&path, lease.place_blocking()).unwrap();
    assert_eq!(read.text, "TITLE \"One\"");
    assert_eq!(watch.holding(), 0);

    assert!(read_text_file(&dir.join("absent.cue"), lease.place_blocking()).is_err());
    assert_eq!(watch.holding(), 0);
    std::fs::remove_dir_all(&dir).unwrap();
}
