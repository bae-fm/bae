use super::*;

#[test]
fn a_path_shows_its_last_name() {
    assert_eq!(name_text(Path::new("/music/Album Title/01.flac")), "01.flac");
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_utf8_shows_escaped() {
    use std::os::unix::ffi::OsStrExt;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"/music/Caf\xE9"));
    assert_eq!(name_text(path), "Caf\\xE9");
}

#[test]
fn a_root_shows_whole() {
    assert_eq!(name_text(Path::new("/")), "/");
}
