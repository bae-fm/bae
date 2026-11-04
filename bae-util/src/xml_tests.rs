use super::*;

#[test]
fn escapes_the_five_special_characters_and_keeps_unicode() {
    assert_eq!(
        xml_escape(r#"a & b < c > d " e ' f é"#),
        "a &amp; b &lt; c &gt; d &quot; e &apos; f é",
    );
}
