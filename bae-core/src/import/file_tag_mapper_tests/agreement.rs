/// A FLAC named `name` in `dir` carrying exactly these Vorbis comments.
fn flac_commented(dir: &Path, name: &str, comments: &[(&str, &str)]) -> PathBuf {
    use lofty::config::WriteOptions;
    let path = dir.join(name);
    fs::copy(
        fixtures_dir().join("flac").join("01 Test Track 1.flac"),
        &path,
    )
    .expect("copy fixture");
    let mut tag = lofty::ogg::tag::VorbisComments::default();
    for (key, value) in comments {
        tag.push((*key).to_string(), (*value).to_string());
    }
    tag.save_to_path(&path, WriteOptions::default())
        .expect("save comments");
    path
}

/// The values the files commented with `files` read as, one file each.
fn read_commented(files: &[&[(&str, &str)]]) -> ReleaseValues {
    let temp = TempDir::new().unwrap();
    let paths: Vec<PathBuf> = files
        .iter()
        .enumerate()
        .map(|(index, comments)| {
            flac_commented(temp.path(), &format!("{:02}.flac", index + 1), comments)
        })
        .collect();
    read_file_tags(&paths, Some("Folder Name")).unwrap()
}

fn album_artist_names(values: &ReleaseValues) -> Vec<&str> {
    values
        .album_credits
        .iter()
        .map(|credit| credit.credit.name.as_str())
        .collect()
}

fn track_artist_names(values: &ReleaseValues, index: usize) -> Option<Vec<&str>> {
    match &values.tracks[index].credits {
        TrackCredits::AlbumArtists => None,
        TrackCredits::Own(credits) => Some(
            credits
                .iter()
                .map(|credit| credit.credit.name.as_str())
                .collect(),
        ),
    }
}

/// A file's two dates name the album by the earlier and the pressing by the
/// later; a lone date names the album alone.
#[test]
fn a_file_s_dates_name_the_album_year_and_the_pressing_year() {
    let values = read_commented(&[&[("DATE", "2005"), ("ORIGINALDATE", "1971")]]);
    assert_eq!(values.album_year, Some(1971));
    assert_eq!(values.pressing.year, Some(2005));

    let values = read_commented(&[&[("DATE", "2005")]]);
    assert_eq!(values.album_year, Some(2005));
    assert_eq!(values.pressing.year, None);
}

/// Files whose dates name the same years agree on them; files that state
/// different years leave both unfilled, never the first file's or the
/// earliest.
#[test]
fn files_agree_on_years_or_leave_them_unfilled() {
    let both: &[(&str, &str)] = &[("DATE", "2005"), ("ORIGINALDATE", "1971")];
    let values = read_commented(&[both, both, &[("TITLE", "Undated")]]);
    assert_eq!(values.album_year, Some(1971));
    assert_eq!(values.pressing.year, Some(2005));

    let values = read_commented(&[&[("DATE", "1971")], &[("DATE", "2005")]]);
    assert_eq!(values.album_year, None);
    assert_eq!(values.pressing.year, None);
}

/// A file naming two artists gives its track two credits, in order.
#[test]
fn a_file_naming_two_artists_gives_its_track_two_credits() {
    let values = read_commented(&[&[
        ("ALBUMARTIST", "Artist Alpha"),
        ("ARTIST", "Artist Alpha"),
        ("ARTIST", "Artist Beta"),
    ]]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(
        track_artist_names(&values, 0),
        Some(vec!["Artist Alpha", "Artist Beta"])
    );
}

/// A track whose ARTIST folds to the album artist's name takes the album's
/// credit, however it is cased.
#[test]
fn a_track_artist_naming_the_album_artist_takes_the_album_s_credit() {
    let values = read_commented(&[
        &[("ALBUMARTIST", "Artist Alpha"), ("ARTIST", "ARTIST ALPHA")],
        &[("ALBUMARTIST", "Artist Alpha"), ("ARTIST", "Artist Beta")],
    ]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(track_artist_names(&values, 0), None);
    assert_eq!(track_artist_names(&values, 1), Some(vec!["Artist Beta"]));
}

/// Files that disagree on the album's title leave it unfilled, not the
/// folder's name; a file stating none does not disagree.
#[test]
fn files_disagreeing_on_the_album_title_leave_it_unfilled() {
    let values = read_commented(&[&[("ALBUM", "Album Alpha")], &[("ALBUM", "Album Beta")]]);
    assert_eq!(values.album_title, "");

    let values = read_commented(&[&[("ALBUM", "Album Alpha")], &[("TITLE", "Untitled album")]]);
    assert_eq!(values.album_title, "Album Alpha");

    let values = read_commented(&[&[("TITLE", "Track Alpha")]]);
    assert_eq!(values.album_title, "Folder Name");
}

/// The album artist is the ALBUMARTIST the files agree on, else the ARTIST
/// they agree on; files naming different artists in both leave it unfilled
/// rather than taking the first file's, and each track keeps its own.
#[test]
fn files_disagreeing_on_the_artist_leave_the_album_artist_unfilled() {
    let values = read_commented(&[&[("ARTIST", "Artist Alpha")], &[("ARTIST", "Artist Alpha")]]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(track_artist_names(&values, 0), None);

    let values = read_commented(&[&[("ARTIST", "Artist Alpha")], &[("ARTIST", "Artist Beta")]]);
    assert!(values.album_credits.is_empty());
    assert_eq!(track_artist_names(&values, 0), Some(vec!["Artist Alpha"]));
    assert_eq!(track_artist_names(&values, 1), Some(vec!["Artist Beta"]));

    let values = read_commented(&[
        &[("ALBUMARTIST", "Artist Alpha"), ("ARTIST", "Artist Gamma")],
        &[("ALBUMARTIST", "Artist Beta"), ("ARTIST", "Artist Gamma")],
    ]);
    assert!(values.album_credits.is_empty());
}

/// Every file flagged a compilation, or naming the various-artists credit as
/// its album artist, makes the album a compilation: it is credited to Various
/// Artists and each track keeps its own artist. One file that says otherwise
/// makes it no compilation.
#[test]
fn a_folder_every_file_of_which_says_compilation_is_one() {
    let values = read_commented(&[
        &[("COMPILATION", "1"), ("ARTIST", "Artist Alpha")],
        &[("ALBUMARTIST", "Various"), ("ARTIST", "Artist Beta")],
    ]);
    assert_eq!(album_artist_names(&values), ["Various Artists"]);
    assert_eq!(track_artist_names(&values, 0), Some(vec!["Artist Alpha"]));
    assert_eq!(track_artist_names(&values, 1), Some(vec!["Artist Beta"]));

    let values = read_commented(&[
        &[("COMPILATION", "1"), ("ARTIST", "Artist Alpha")],
        &[("ARTIST", "Artist Alpha")],
    ]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(track_artist_names(&values, 0), None);
}

/// CUE sheets agree like files: a TITLE they disagree on is left unfilled,
/// a PERFORMER they agree on is the album artist, and a sheet's `REM DATE`
/// years are read by the year rule.
#[test]
fn cue_sheets_agree_on_the_album_like_files() {
    let mut disc1 = cue_sheet("Album Alpha", vec![cue_track(1, "One")]);
    disc1.date = Some("1971 / 2005".to_string());
    let mut disc2 = cue_sheet("Album Beta", vec![cue_track(1, "Two")]);
    disc2.date = Some("1971 / 2005".to_string());
    let values = map_cue_folder(vec![disc1, disc2], Some("Folder Name"));
    assert_eq!(values.album_title, "");
    assert_eq!(album_artist_names(&values), ["Artist Name"]);
    assert_eq!(values.album_year, Some(1971));
    assert_eq!(values.pressing.year, Some(2005));

    let disc1 = cue_sheet("Album Alpha", vec![cue_track(1, "One")]);
    let mut disc2 = cue_sheet("Album Alpha", vec![cue_track(1, "Two")]);
    disc2.performer = Some("Artist Other".to_string());
    let values = map_cue_folder(vec![disc1, disc2], None);
    assert_eq!(values.album_title, "Album Alpha");
    assert!(values.album_credits.is_empty());
}
