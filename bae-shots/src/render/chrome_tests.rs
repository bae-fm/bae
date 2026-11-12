use super::*;

#[test]
fn each_published_platform_has_a_url_and_a_folder() {
    let mac = Platform::of("macos", "aarch64").unwrap();
    assert_eq!(
        mac.url("157.0.8103.78"),
        "https://storage.googleapis.com/chrome-for-testing-public/157.0.8103.78/mac-arm64/chrome-mac-arm64.zip"
    );
    assert_eq!(
        mac.folder(Path::new("/cache"), "157.0.8103.78"),
        Path::new("/cache/157.0.8103.78-mac-arm64")
    );
    assert!(mac.executable.starts_with("chrome-mac-arm64/"));
    let linux = Platform::of("linux", "x86_64").unwrap();
    assert_eq!(linux.executable, "chrome-linux64/chrome");
    assert!(linux.url("1.2.3").ends_with("/1.2.3/linux64/chrome-linux64.zip"));
}

#[test]
fn an_unpublished_platform_has_none() {
    assert_eq!(Platform::of("windows", "x86_64"), None);
    assert_eq!(Platform::of("linux", "aarch64"), None);
}
