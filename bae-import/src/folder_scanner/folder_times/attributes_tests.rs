use super::*;

/// One entry as `getattrlistbulk` packs it: returned set, then the
/// attributes it names in order, the name's bytes at the end.
fn packed(error: Option<u32>, name: &str, dates: &[(libc::attrgroup_t, i64, i64)]) -> Vec<u8> {
    let mut returned = libc::ATTR_CMN_RETURNED_ATTRS | libc::ATTR_CMN_NAME;
    let mut body = Vec::new();
    if let Some(error) = error {
        returned |= ATTR_CMN_ERROR;
        body.extend_from_slice(&error.to_ne_bytes());
    }
    let reference_at = body.len();
    body.extend_from_slice(&[0; 8]);
    for (attribute, seconds, nanos) in dates {
        returned |= attribute;
        body.extend_from_slice(&seconds.to_ne_bytes());
        body.extend_from_slice(&nanos.to_ne_bytes());
    }
    let name_at = body.len();
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    while body.len() % 4 != 0 {
        body.push(0);
    }
    let offset = (name_at - reference_at) as u32;
    body[reference_at..reference_at + 4].copy_from_slice(&offset.to_ne_bytes());
    body[reference_at + 4..reference_at + 8]
        .copy_from_slice(&(name.len() as u32 + 1).to_ne_bytes());
    let mut entry = Vec::new();
    entry.extend_from_slice(&((4 + 20 + body.len()) as u32).to_ne_bytes());
    entry.extend_from_slice(&returned.to_ne_bytes());
    entry.extend_from_slice(&[0; 16]);
    entry.extend_from_slice(&body);
    entry
}

#[test]
fn decodes_the_dates_an_entry_returns_and_none_for_the_rest() {
    let entry = packed(
        Some(0),
        "Folder",
        &[(libc::ATTR_CMN_CRTIME, 20, 0), (libc::ATTR_CMN_MODTIME, 30, 500)],
    );
    assert_eq!(
        decode(&entry).unwrap(),
        Packed {
            length: entry.len(),
            error: None,
            name: Some("Folder".into()),
            times: FolderTimes {
                added: None,
                created: Some(UNIX_EPOCH + Duration::from_secs(20)),
                modified: Some(UNIX_EPOCH + Duration::new(30, 500)),
            },
        }
    );
}

#[test]
fn an_entry_says_the_error_that_kept_its_dates() {
    let entry = packed(Some(libc::EACCES as u32), "Folder", &[]);
    let packed = decode(&entry).unwrap();
    assert_eq!(packed.length, entry.len());
    assert_eq!(packed.error, Some(libc::EACCES));
}

#[test]
fn a_cut_off_entry_or_one_whose_date_is_no_time_is_invalid() {
    let entry = packed(None, "Folder", &[(libc::ATTR_CMN_ADDEDTIME, 1, 0)]);
    assert!(decode(&entry[..entry.len() - 8]).is_err());
    let entry = packed(None, "Folder", &[(libc::ATTR_CMN_ADDEDTIME, 1, 1_000_000_000)]);
    assert!(decode(&entry).is_err());
}

/// One folder's date added as `getattrlist` packs it, every asked
/// attribute in place: the returned set says whether it is a date.
#[test]
fn a_folders_own_answer_is_a_date_only_where_the_returned_set_says_so() {
    let mut buffer = [0u8; 40];
    buffer[..4].copy_from_slice(&40u32.to_ne_bytes());
    assert_eq!(decode(&buffer).unwrap().times.added, None);
    buffer[4..8].copy_from_slice(&libc::ATTR_CMN_ADDEDTIME.to_ne_bytes());
    assert_eq!(decode(&buffer).unwrap().times.added, Some(UNIX_EPOCH));
    buffer[24..32].copy_from_slice(&123i64.to_ne_bytes());
    buffer[32..40].copy_from_slice(&456_000_000i64.to_ne_bytes());
    assert_eq!(
        decode(&buffer).unwrap().times.added,
        Some(UNIX_EPOCH + Duration::from_millis(123_456))
    );
    buffer[32..40].copy_from_slice(&1_000_000_000i64.to_ne_bytes());
    assert!(decode(&buffer).is_err());
    buffer[..4].copy_from_slice(&24u32.to_ne_bytes());
    assert!(decode(&buffer).is_err());
}

#[test]
fn reads_a_folders_date_added_and_fails_for_a_missing_one() {
    let directory = tempfile::tempdir().unwrap();
    assert!(added(directory.path()).unwrap().is_some());
    assert!(added(&directory.path().join("missing")).is_err());
}

#[test]
fn reads_the_names_and_dates_the_listing_holds() {
    let directory = tempfile::tempdir().unwrap();
    let mut names: Vec<String> = (0..300).map(|index| format!("Folder {index:03}")).collect();
    for name in &names {
        std::fs::create_dir(directory.path().join(name)).unwrap();
    }
    std::fs::write(directory.path().join("file.txt"), b"x").unwrap();
    names.push("file.txt".to_string());
    let entries = read_directory(directory.path()).unwrap();
    let mut read_names: Vec<String> =
        entries.keys().map(|name| name.to_string_lossy().into_owned()).collect();
    read_names.sort();
    names.sort();
    assert_eq!(read_names, names);
    let metadata = std::fs::metadata(directory.path().join("Folder 000")).unwrap();
    let times = entries[std::ffi::OsStr::new("Folder 000")];
    assert_eq!(times.created, metadata.created().ok());
    assert_eq!(times.modified, metadata.modified().ok());
    assert!(times.added.is_some());
}

#[test]
fn a_missing_directory_fails() {
    let directory = tempfile::tempdir().unwrap();
    assert!(read_directory(&directory.path().join("missing")).is_err());
}
