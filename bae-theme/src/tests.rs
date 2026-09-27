use super::*;

const THEME: &str = include_str!("../../design/theme.toml");

#[test]
fn the_shipped_theme_is_valid() {
    let theme = Theme::from_toml(THEME).unwrap();
    assert_eq!(theme.tones[0].name, "neutral");
    assert_eq!(theme.accents[0].name, "blue");
}

#[test]
fn colours_read_with_and_without_alpha() {
    assert_eq!(Argb::parse("#F5F5F5"), Ok(Argb(0xFFF5_F5F5)));
    assert_eq!(Argb::parse("#1A000000"), Ok(Argb(0x1A00_0000)));
    assert!(Argb::parse("F5F5F5").is_err());
    assert!(Argb::parse("#F5F5F").is_err());
    assert!(Argb::parse("#+5F5F5F").is_err());
}

fn theme_with(tones: &str, semantics: &str) -> Result<Theme, Vec<String>> {
    Theme::from_toml(&format!(
        "[opacity]\ntint = 0.14\n\n{tones}\n[[accents]]\nname = \"blue\"\nlight = \"#000000\"\ndark = \"#000000\"\nfill = \"#000000\"\n\n{semantics}"
    ))
}

const SEMANTICS: &str =
    "[semantics.light]\ndanger = \"#000000\"\n[semantics.dark]\ndanger = \"#000000\"\n";

#[test]
fn a_tone_missing_a_surface_is_refused() {
    let tones = "[[tones]]\nname = \"neutral\"\n[tones.light]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [[tones]]\nname = \"slate\"\n[tones.light]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\n";
    let problems = theme_with(tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        ["tone slate.dark lacks `well`, which tone neutral.light has"]
    );
}

#[test]
fn semantics_must_match_across_appearances() {
    let tones = "[[tones]]\nname = \"neutral\"\n[tones.light]\nbackground = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\n";
    let semantics =
        "[semantics.light]\ndanger = \"#000000\"\n[semantics.dark]\nwarning = \"#000000\"\n";
    let problems = theme_with(tones, semantics).unwrap_err();
    assert_eq!(
        problems,
        [
            "semantics.dark lacks `danger`, which semantics.light has",
            "semantics.dark has `warning`, which semantics.light lacks",
        ]
    );
}

#[test]
fn names_must_become_identifiers() {
    let tones = "[[tones]]\nname = \"Neutral\"\n[tones.light]\nBack-ground = \"#000000\"\n\
                 [tones.dark]\nBack-ground = \"#000000\"\n";
    let problems = theme_with(tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        [
            "tone `Neutral`: a name is lowercase letters",
            "role `Back-ground`: a role name is lowerCamelCase",
        ]
    );
}

#[test]
fn both_platforms_carry_every_role() {
    let theme = Theme::from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for tone in &theme.tones {
        assert!(swift.contains(&format!("    case {}\n", tone.name)));
        assert!(kotlin.contains(&format!("    {},\n", tone.name.to_uppercase())));
    }
    for role in theme.tones[0]
        .surfaces
        .light
        .keys()
        .chain(theme.semantics.light.keys())
    {
        assert!(
            swift.contains(&format!("    let {role}: Color\n"))
                || swift.contains(&format!("    public static let {role} = Color(\n")),
            "{role}"
        );
        assert!(
            kotlin.contains(&format!("    val {role}: Color,\n")),
            "{role}"
        );
    }
}

#[test]
fn both_platforms_carry_every_opacity() {
    let theme = Theme::from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.opacity {
        assert!(swift.contains(&format!(
            "    public static let {role}: Double = {value:?}\n"
        )));
        assert!(kotlin.contains(&format!("    const val {role}: Float = {value:?}f\n")));
    }
}

#[test]
fn the_launch_background_is_the_first_tones() {
    let theme = Theme::from_toml(THEME).unwrap();
    let resources = android::launch_resources(&theme).unwrap();
    assert_eq!(resources[0].0, "res/values/theme_colors.xml");
    assert!(resources[0].1.contains(">#FFF5F5F5<"));
    assert_eq!(resources[1].0, "res/values-night/theme_colors.xml");
    assert!(resources[1].1.contains(">#FF1B1B1D<"));
}
