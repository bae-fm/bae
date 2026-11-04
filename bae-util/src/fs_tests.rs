use super::*;

#[test]
fn an_io_error_at_a_path_names_the_path_and_keeps_its_kind() {
    let error = io_error_at(
        std::path::Path::new("/library/pointer"),
        std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    );
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(error.to_string().starts_with("/library/pointer: "), "{error}");
}
