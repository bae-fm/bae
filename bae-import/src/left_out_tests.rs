use super::*;

/// The crash reporter is told the step and the kind of failure, never the
/// path, which the error's text names.
#[test]
fn an_input_that_fails_is_left_out_and_reported_without_its_path() {
    let (inputs, bugs) = reporting_inputs();
    let path = Path::new("/music/Invented Artist/notes.txt");

    let read: Option<()> = inputs.read(InputRead::TextFile, path, || {
        Err(LeftOut::io(&std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is locked", path.display()),
        )))
    });

    assert_eq!(read, None);
    assert!(inputs.left_out(path));
    assert!(!inputs.all_read());
    assert_eq!(
        bugs.reports(),
        vec![(
            "identify: text file".to_string(),
            "reading it failed: PermissionDenied".to_string()
        )]
    );
}

/// A read that panics leaves its input out as one that fails does. The panic
/// hook reports the panic, so the read reports nothing more.
#[test]
fn an_input_whose_read_panics_is_left_out_and_the_next_one_is_read() {
    let (inputs, bugs) = reporting_inputs();

    let read: Option<()> = inputs
        .read(InputRead::AudioTags, Path::new("01.flac"), || panic!("a tag reader that panics"));

    assert_eq!(read, None);
    assert!(inputs.left_out(Path::new("01.flac")));
    assert_eq!(inputs.read(InputRead::AudioTags, Path::new("02.flac"), || Ok(2)), Some(2));
    assert!(!inputs.left_out(Path::new("02.flac")));
    assert_eq!(bugs.reports(), Vec::new());
}
