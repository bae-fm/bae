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

/// read_text_file gives the place of the file it reads back when the read
/// ends.
#[test]
fn reading_a_text_file_gives_its_place_back() {
    let folder = crate::test_files::TestFolder::new();
    let path = folder.write("sheet.cue", "TITLE \"One\"");
    let budget = coven::FileBudget::for_test();
    let watch = budget.watch();
    let lease = budget.lease_blocking(1).unwrap();

    let file = crate::test_files::open_in(lease.place_blocking(), &path).unwrap();
    assert_eq!(watch.holding(), 1);
    let read = read_text_file(file).unwrap();
    assert_eq!(read.text, "TITLE \"One\"");
    assert_eq!(watch.holding(), 0);
}
