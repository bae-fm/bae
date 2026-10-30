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

fn theme_with(standard: &str, tones: &str, semantics: &str) -> Result<Theme, Vec<String>> {
    Theme::from_toml(&format!(
        "{standard}\n[opacity]\ntint = 0.14\n\n[radius]\nchip = 5\n\n[space]\nrelated = 8\n\n[icon.small]\nweight = \"semibold\"\nmacos = 11\nios = 13\nandroid = 16\n\n\
         [size.hitTarget]\nmacos = 28\nios = 44\nandroid = 44\n\n{TEXT}\n{tones}\n[[accents]]\nname = \"blue\"\nlight = \"#000000\"\ndark = \"#000000\"\nfill = \"#000000\"\n\n{semantics}"
    ))
}

const STANDARD: &str = "[standard]\ntone = \"neutral\"\naccent = \"blue\"\n";

const TEXT: &str = "[text.body]\nweight = \"regular\"\nmacos = 13\nios = \"body\"\nandroid = 14\n";

const SEMANTICS: &str =
    "[semantics.light]\ndanger = \"#000000\"\n[semantics.dark]\ndanger = \"#000000\"\n";

#[test]
fn a_tone_missing_a_surface_is_refused() {
    let tones = "[[tones]]\nname = \"neutral\"\n[tones.light]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [[tones]]\nname = \"slate\"\n[tones.light]\nbackground = \"#000000\"\nwell = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\n";
    let problems = theme_with(STANDARD, tones, SEMANTICS).unwrap_err();
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
    let problems = theme_with(STANDARD, tones, semantics).unwrap_err();
    assert_eq!(
        problems,
        [
            "semantics.dark lacks `danger`, which semantics.light has",
            "semantics.dark has `warning`, which semantics.light lacks",
        ]
    );
}

#[test]
fn the_standard_choices_must_be_listed() {
    let tones = "[[tones]]\nname = \"neutral\"\n[tones.light]\nbackground = \"#000000\"\n\
                 [tones.dark]\nbackground = \"#000000\"\n";
    let standard = "[standard]\ntone = \"dusk\"\naccent = \"mauve\"\n";
    let problems = theme_with(standard, tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        [
            "the standard tone `dusk` is not a listed tone",
            "the standard accent `mauve` is not a listed accent",
        ]
    );
}

#[test]
fn both_platforms_carry_the_standard_choices() {
    let theme = Theme::from_toml(THEME).unwrap();
    let tone = &theme.tones[theme.standard.tone].name;
    let accent = &theme.accents[theme.standard.accent].name;
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    assert!(swift.contains(&format!(
        "    public static let standard = SurfaceTone.{tone}\n"
    )));
    assert!(swift.contains(&format!(
        "    public static let standard = AccentChoice.{accent}\n"
    )));
    assert!(kotlin.contains(&format!("        val STANDARD = {}\n", tone.to_uppercase())));
    assert!(kotlin.contains(&format!(
        "        val STANDARD = {}\n",
        accent.to_uppercase()
    )));
}

#[test]
fn names_must_become_identifiers() {
    let tones = "[[tones]]\nname = \"Neutral\"\n[tones.light]\nBack-ground = \"#000000\"\n\
                 [tones.dark]\nBack-ground = \"#000000\"\n";
    let standard = "[standard]\ntone = \"Neutral\"\naccent = \"blue\"\n";
    let problems = theme_with(standard, tones, SEMANTICS).unwrap_err();
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
fn both_platforms_carry_every_radius() {
    let theme = Theme::from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.radius {
        assert!(swift.contains(&format!(
            "    public static let {role}: CGFloat = {value}\n"
        )));
        assert!(kotlin.contains(&format!("    val {role} = {value}.dp\n")));
    }
}

#[test]
fn a_text_role_needs_every_platforms_size() {
    let theme = Theme::from_toml(&THEME.replace(
        "[text.body]\nweight = \"regular\"\nmacos = 13\n",
        "[text.body]\nweight = \"regular\"\n",
    ));
    assert_eq!(theme.unwrap_err(), ["text body has no macos size"]);
}

#[test]
fn an_ios_size_names_a_dynamic_type_style() {
    let theme = Theme::from_toml(&THEME.replace(
        "[text.body]\nweight = \"regular\"\nmacos = 13\nios = \"body\"",
        "[text.body]\nweight = \"regular\"\nmacos = 13\nios = \"huge\"",
    ));
    assert_eq!(
        theme.unwrap_err(),
        ["text body: `huge` is not a Dynamic Type style"]
    );
}

#[test]
fn both_platforms_carry_every_text_role() {
    let theme = Theme::from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for role in theme.text.keys() {
        assert!(swift.contains(&format!("    public static let {role} = ThemeText(\n")));
        assert!(kotlin.contains(&format!("        val {role} =\n")));
    }
}

#[test]
fn an_icon_needs_every_platforms_size() {
    let theme = Theme::from_toml(&THEME.replace(
        "[icon.small]\nweight = \"semibold\"\nmacos = 11\nios = 13\nandroid = 16\n",
        "[icon.small]\nweight = \"semibold\"\nmacos = 11\nios = 13\n",
    ));
    assert_eq!(theme.unwrap_err(), ["icon small has no android length"]);
}

#[test]
fn both_platforms_carry_every_space_icon_and_size() {
    let theme = Theme::from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.space {
        assert!(swift.contains(&format!(
            "    public static let {role}: CGFloat = {value}\n"
        )));
        assert!(kotlin.contains(&format!("    val {role} = {value}.dp\n")));
    }
    for (role, icon) in &theme.icon {
        assert!(swift.contains(&format!("    public static let {role} = ThemeIcon(\n")));
        assert!(kotlin.contains(&format!("    val {role} = {}.dp\n", icon.sizes.android)));
    }
    for (role, lengths) in &theme.size {
        assert!(swift.contains(&format!(
            "    public static let {role}: CGFloat = .platform(\n"
        )));
        assert!(kotlin.contains(&format!("    val {role} = {}.dp\n", lengths.android)));
    }
}

#[test]
fn the_launch_background_is_the_standard_tones() {
    let theme = Theme::from_toml(THEME).unwrap();
    let resources = android::launch_resources(&theme).unwrap();
    // Plum's backgrounds.
    assert_eq!(resources[0].0, "res/values/theme_colors.xml");
    assert!(resources[0].1.contains(">#FFEFE4F1<"));
    assert_eq!(resources[1].0, "res/values-night/theme_colors.xml");
    assert!(resources[1].1.contains(">#FF201B25<"));
}
