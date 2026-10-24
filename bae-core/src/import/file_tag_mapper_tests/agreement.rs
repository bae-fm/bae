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
/// different years give the album the first file's, in play order. A file
/// stating none counts for none.
#[test]
fn files_give_the_years_they_agree_on_else_the_first_file_s() {
    let both: &[(&str, &str)] = &[("DATE", "2005"), ("ORIGINALDATE", "1971")];
    let values = read_commented(&[&[("TITLE", "Undated")], both, both]);
    assert_eq!(values.album_year, Some(1971));
    assert_eq!(values.pressing.year, Some(2005));

    let values = read_commented(&[&[("DATE", "1963")], &[("DATE", "1987")], &[("DATE", "1987")]]);
    assert_eq!(values.album_year, Some(1963));
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

/// Files that agree on the album's title give it as they state it; files
/// titling it differently give it the first file's title, without the disc
/// marker it ends with. A file stating none counts for none, and with none
/// stated anywhere the folder's name is the title.
#[test]
fn files_give_the_title_they_agree_on_else_the_first_file_s() {
    let values = read_commented(&[
        &[("ALBUM", "Album Title CD1")],
        &[("ALBUM", "Album Title CD1")],
    ]);
    assert_eq!(values.album_title, "Album Title CD1");

    let values = read_commented(&[
        &[("ALBUM", "Album Title (Disc 1)")],
        &[("ALBUM", "Album Title (Disc 2)")],
    ]);
    assert_eq!(values.album_title, "Album Title");

    let values = read_commented(&[
        &[("ALBUM", "Album Title")],
        &[("ALBUM", "Album Title / Other Album")],
        &[("ALBUM", "Other Album")],
    ]);
    assert_eq!(values.album_title, "Album Title");

    let values = read_commented(&[&[("TITLE", "Untitled album")], &[("ALBUM", "Album Alpha")]]);
    assert_eq!(values.album_title, "Album Alpha");

    let values = read_commented(&[&[("TITLE", "Track Alpha")]]);
    assert_eq!(values.album_title, "Folder Name");
}

/// The disc marker a set's first disc ends its title with, and the
/// separator before it, are not the album's: brackets or none, any case, a
/// space before the number or none. A volume is no disc, and a title that is
/// only a marker stays.
#[test]
fn the_first_disc_s_title_drops_its_disc_marker() {
    for (title, album) in [
        ("Album Title (cd 1)", "Album Title"),
        ("Album Title: Subtitle (cd 1)", "Album Title: Subtitle"),
        ("Album Title (Disc 1)", "Album Title"),
        ("Album Title [disc 1]", "Album Title"),
        ("Album Title [disc  09]", "Album Title"),
        ("Album Title CD1", "Album Title"),
        ("Album Title CD 1", "Album Title"),
        ("Album Title - Disc 1", "Album Title"),
        ("Album Title Disc 1", "Album Title"),
        ("Album Title_Anthology (1974-84) D2", "Album Title_Anthology (1974-84)"),
        ("Album Title, Vol.1 CD1", "Album Title, Vol.1"),
        ("Album Title ", "Album Title"),
        ("Album Title Volume 1", "Album Title Volume 1"),
        ("Album Title (1974-84)", "Album Title (1974-84)"),
        ("Album Band 2", "Album Band 2"),
        ("Disc 1", "Disc 1"),
    ] {
        assert_eq!(without_disc_marker(title), album, "{title:?}");
    }
}

/// A set's discs, each titled after its own disc, title the album after the
/// first; volumes keep the first disc's volume.
#[test]
fn a_set_titled_disc_by_disc_takes_the_first_disc_s_title() {
    let values = read_commented(&[
        &[("ALBUM", "Album Title [disc 1]")],
        &[("ALBUM", "Album Title [disc 02]")],
        &[("ALBUM", "Album Title [disc  09]")],
    ]);
    assert_eq!(values.album_title, "Album Title");

    let values = read_commented(&[
        &[("ALBUM", "Album Title Volume 1")],
        &[("ALBUM", "Album Title Volume 2")],
    ]);
    assert_eq!(values.album_title, "Album Title Volume 1");
}

/// The album artist is the ALBUMARTIST the files agree on, else the first
/// file's; where no file names one, the ARTIST they agree on, else the first
/// file's. Each track naming another keeps its own.
#[test]
fn files_naming_different_artists_give_the_album_the_first_file_s() {
    let values = read_commented(&[&[("ARTIST", "Artist Alpha")], &[("ARTIST", "Artist Alpha")]]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(track_artist_names(&values, 0), None);

    let values = read_commented(&[
        &[("ARTIST", "Artist Beta")],
        &[("ARTIST", "Artist Alpha")],
        &[("ARTIST", "Artist Alpha")],
    ]);
    assert_eq!(album_artist_names(&values), ["Artist Beta"]);
    assert_eq!(track_artist_names(&values, 0), None);
    assert_eq!(track_artist_names(&values, 1), Some(vec!["Artist Alpha"]));

    let values = read_commented(&[
        &[("ALBUMARTIST", "Artist Alpha"), ("ARTIST", "Artist Gamma")],
        &[("ALBUMARTIST", "Artist Beta"), ("ARTIST", "Artist Gamma")],
    ]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
}

/// A compilation whose files name neither an album artist nor say they are
/// a compilation is credited to its first track's artist, and every other
/// track keeps its own.
#[test]
fn an_unflagged_compilation_is_credited_to_its_first_track_s_artist() {
    let values = read_commented(&[
        &[("ARTIST", "Artist Alpha")],
        &[("ARTIST", "Artist Beta")],
        &[("ARTIST", "Artist Gamma")],
    ]);
    assert_eq!(album_artist_names(&values), ["Artist Alpha"]);
    assert_eq!(track_artist_names(&values, 0), None);
    assert_eq!(track_artist_names(&values, 1), Some(vec!["Artist Beta"]));
    assert_eq!(track_artist_names(&values, 2), Some(vec!["Artist Gamma"]));
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

/// CUE sheets give the album like files: the TITLE they agree on, else the
/// first sheet's without its disc marker; the PERFORMER they agree on, else
/// the first sheet's; their `REM DATE` years by the year rule.
#[test]
fn cue_sheets_give_the_album_like_files() {
    let mut disc1 = cue_sheet("Album Title (cd 1)", vec![cue_track(1, "One")]);
    disc1.date = Some("1971 / 2005".to_string());
    let mut disc2 = cue_sheet("Album Title (cd 2)", vec![cue_track(1, "Two")]);
    disc2.date = Some("1971 / 2005".to_string());
    let values = map_cue_folder(vec![disc1, disc2], Some("Folder Name"));
    assert_eq!(values.album_title, "Album Title");
    assert_eq!(album_artist_names(&values), ["Artist Name"]);
    assert_eq!(values.album_year, Some(1971));
    assert_eq!(values.pressing.year, Some(2005));

    let disc1 = cue_sheet("Album Alpha", vec![cue_track(1, "One")]);
    let mut disc2 = cue_sheet("Album Alpha", vec![cue_track(1, "Two")]);
    disc2.performer = Some("Artist Other".to_string());
    let values = map_cue_folder(vec![disc1, disc2], None);
    assert_eq!(values.album_title, "Album Alpha");
    assert_eq!(album_artist_names(&values), ["Artist Name"]);
}
