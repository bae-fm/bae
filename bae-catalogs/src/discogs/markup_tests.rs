use super::shown_text;

#[test]
fn a_named_link_shows_its_name() {
    assert_eq!(shown_text("Pressed by [l=Label Name]."), "Pressed by Label Name.");
    assert_eq!(shown_text("Mixed by [a=Artist Name]."), "Mixed by Artist Name.");
}

#[test]
fn a_url_shows_its_text() {
    assert_eq!(shown_text("See [url=https://example.com/page]the page[/url]."), "See the page.");
    assert_eq!(shown_text("See [url]the page[/url]."), "See the page.");
}

#[test]
fn formatting_tags_go() {
    assert_eq!(shown_text("[b]Limited[/b] [i]first[/i] [u]press[/u]"), "Limited first press");
}

#[test]
fn an_id_only_reference_is_kept() {
    let note = "Cut at [l123], see [a237886], [r123], [m123], [r=123] and [m=123].";
    assert_eq!(shown_text(note), note);
}

#[test]
fn other_bracketed_text_and_stray_brackets_are_kept() {
    let note = "Matrix [side A] runs [ out and ends [";
    assert_eq!(shown_text(note), note);
}
