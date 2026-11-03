use super::*;

#[test]
fn trailing_brackets_leave_the_words() {
    assert_eq!(strip_trailing_brackets("Album Title [XX34b]"), "Album Title");
    assert_eq!(strip_trailing_brackets("Album Title (Deluxe) [2020]"), "Album Title");
    assert_eq!(strip_trailing_brackets("[XX34b]"), "");
    assert_eq!(strip_trailing_brackets("Album Title"), "Album Title");
}

/// A bracket that opens a name, or sits inside it, is part of it.
#[test]
fn only_trailing_brackets_go() {
    assert_eq!(
        strip_trailing_brackets("(Leading Words) Album Title?"),
        "(Leading Words) Album Title?"
    );
    assert_eq!(strip_trailing_brackets("Album (Middle) Title"), "Album (Middle) Title");
}

#[test]
fn an_album_title_is_bare_unless_it_is_nothing_but_brackets() {
    assert_eq!(bare_album_title("Album Title (Remastered)"), "Album Title");
    assert_eq!(bare_album_title("[Untitled]"), "[Untitled]");
}

#[test]
fn a_track_title_loses_every_bracketed_part() {
    assert_eq!(track_title_key("Track Title (Remastered)"), "tracktitle");
    assert_eq!(track_title_key("Track [Live] Title (feat. Guest)"), "tracktitle");
    assert_eq!(track_title_key("(Intro)"), "");
}

#[test]
fn an_album_title_s_words_leave_out_its_tails_and_stop_words() {
    assert_eq!(album_title_words("The Album of Titles (Remastered)"), vec!["album", "titles"]);
}
