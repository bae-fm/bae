use super::*;

const THEME: &str = include_str!("../../design/theme.toml");
const TOKENS: &str = include_str!("../../design/system/tokens.json");

fn from_toml(source: &str) -> Result<Theme, Vec<String>> {
    Theme::from_sources(source, TOKENS)
}

/// One color token of tokens.json.
fn token(name: &str, light: &str, dark: &str) -> String {
    format!("{{\"name\": \"{name}\", \"value\": {{\"light\": \"{light}\", \"dark\": \"{dark}\"}}}}")
}

/// tokens.json with these color tokens and one text role, `body`, on both
/// platforms.
fn tokens_json(tokens: &[String]) -> String {
    format!(
        "{{\"color\": {{\"tokens\": [{}]}}, \"type\": {{\"groups\": {BODY}}}, {LENGTHS}}}",
        tokens.join(", ")
    )
}

/// The text style groups of a tokens.json with one text role, `body`, on
/// both platforms.
const BODY: &str = r#"[
  { "family": "sans", "styles": [{ "name": "body", "role": "body", "fontSize": "13px", "fontWeight": 400, "ios": "body" }] },
  { "family": "android", "styles": [{ "name": "android-body", "role": "body", "fontSize": "14px", "fontWeight": 400, "lineHeight": "20px" }] }
]"#;

/// The length families of a tokens.json: one radius, `chip`, one spacing,
/// `related`, an icon role on both platforms, `small`, one on Android alone,
/// `androidTransport`, and one size role, `hitTarget`.
const LENGTHS: &str = r#""radius": { "tokens": [{ "name": "radius-chip", "role": "chip", "value": "5px" }] },
  "spacing": { "tokens": [{ "name": "space-related", "role": "related", "value": "8px" }] },
  "icon": { "tokens": [{ "name": "icon-small", "role": "small", "value": "11px", "ios": "13px", "weight": "semibold" }] },
  "icon-android": { "tokens": [{ "name": "icon-android-small", "role": "small", "value": "16px" }, { "name": "icon-android-transport", "role": "androidTransport", "value": "36px" }] },
  "size": { "tokens": [{ "name": "size-hit-target", "role": "hitTarget", "value": "28px", "ios": "44px" }, { "name": "size-control", "value": "24px" }] },
  "size-android": { "tokens": [{ "name": "size-android-hit-target", "role": "hitTarget", "value": "44px" }] }"#;

/// The shipped tokens.json with <groups> as its text style groups.
fn tokens_with_text(groups: &str) -> String {
    let mut tokens: serde_json::Value = serde_json::from_str(TOKENS).unwrap();
    tokens["type"]["groups"] = serde_json::from_str(groups).unwrap();
    tokens.to_string()
}

/// The theme with the shipped theme.toml and <groups> as tokens.json's text
/// style groups.
fn theme_with_text(groups: &str) -> Result<Theme, Vec<String>> {
    Theme::from_sources(THEME, &tokens_with_text(groups))
}

/// The tokens of tone `name`, one per role, all black.
fn tone(name: &str, roles: &[&str]) -> Vec<String> {
    roles.iter().map(|role| token(&format!("tone-{name}-{role}"), "#000000", "#000000")).collect()
}

/// tokens.json with the tone `plum` and only the accent `name`, in the given
/// colours.
fn tokens_with_accent(
    name: &str,
    text: &str,
    fill: &str,
    on_fill: &str,
    selection: &str,
) -> String {
    let accent = [
        token(&format!("accent-{name}"), text, text),
        token(&format!("accent-{name}-fill"), fill, fill),
        token(&format!("on-accent-{name}-fill"), on_fill, on_fill),
        token(&format!("accent-{name}-selection"), selection, selection),
    ];
    tokens_json(&[tone("plum", &["background"]), accent.to_vec()].concat())
}

#[test]
fn the_shipped_theme_is_valid() {
    let theme = from_toml(THEME).unwrap();
    assert_eq!(theme.tones[0].name, "neutral");
    let names: Vec<&str> = theme.accents.iter().map(|accent| accent.name.as_str()).collect();
    assert_eq!(names, ["blue", "orchid", "pink", "coral", "gold", "lime", "mint", "sky"]);
    assert_eq!(theme.accents[theme.standard.accent].name, "pink");
}

#[test]
fn an_accent_reads_its_four_tokens() {
    let theme = from_toml(THEME).unwrap();
    let blue = &theme.accents[0];
    assert_eq!(blue.light, Argb(0xFF42_60C4));
    assert_eq!(blue.dark, Argb(0xFF99_B5FE));
    assert_eq!(blue.fill, Argb(0xFF8C_AAFE));
    assert_eq!(blue.on_fill, Argb(0xFF17_2455));
    assert_eq!(blue.selection, Argb(0xFF42_60C4));
}

#[test]
fn an_accent_missing_a_token_is_refused() {
    let tokens = tokens_with_accent("blue", "#000001", "#000002", "#000003", "#000004")
        .replace("accent-blue-selection", "accent-blue-selected");
    let problems =
        Theme::from_sources(&THEME.replace("accent = \"pink\"", "accent = \"blue\""), &tokens)
            .unwrap_err();
    assert_eq!(
        problems[0],
        "tokens.json has no `accent-blue-selection`, which accent `blue` needs"
    );
}

#[test]
fn a_swatch_that_differs_between_appearances_is_refused() {
    let tokens = tokens_with_accent("blue", "#000001", "#000002", "#000003", "#000004")
        .replace("\"#000002\"}", "\"#0000FF\"}");
    let problems =
        Theme::from_sources(&THEME.replace("accent = \"pink\"", "accent = \"blue\""), &tokens)
            .unwrap_err();
    assert_eq!(
        problems,
        ["tokens.json `accent-blue-fill` differs between appearances; the apps draw it as one colour"]
    );
}

#[test]
fn colours_read_with_and_without_alpha() {
    assert_eq!(Argb::parse("#F5F5F5"), Ok(Argb(0xFFF5_F5F5)));
    assert_eq!(Argb::parse("#1A000000"), Ok(Argb(0x1A00_0000)));
    assert!(Argb::parse("F5F5F5").is_err());
    assert!(Argb::parse("#F5F5F").is_err());
    assert!(Argb::parse("#+5F5F5F").is_err());
}

fn theme_with(standard: &str, tones: &[String], semantics: &str) -> Result<Theme, Vec<String>> {
    let accent =
        ["accent-blue", "accent-blue-fill", "on-accent-blue-fill", "accent-blue-selection"]
            .map(|name| token(name, "#000000", "#000000"));
    Theme::from_sources(
        &format!("{standard}\n[opacity]\ntint = 0.14\n\n{semantics}"),
        &tokens_json(&[tones, &accent[..]].concat()),
    )
}

const STANDARD: &str = "[standard]\ntone = \"neutral\"\naccent = \"blue\"\n";

const SEMANTICS: &str =
    "[semantics.light]\ndanger = \"#000000\"\n[semantics.dark]\ndanger = \"#000000\"\n";

#[test]
fn a_tone_missing_a_surface_is_refused() {
    let tones = [tone("neutral", &["background", "well"]), tone("slate", &["background"])].concat();
    let problems = theme_with(STANDARD, &tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        [
            "tone slate.light lacks `well`, which tone neutral.light has",
            "tone slate.dark lacks `well`, which tone neutral.light has",
        ]
    );
}

#[test]
fn a_tone_token_needs_both_appearances() {
    let tones = [
        tone("neutral", &["background"]),
        vec!["{\"name\": \"tone-neutral-field-hover\", \"value\": {\"light\": \"#000000\"}}"
            .to_string()],
    ]
    .concat();
    let problems = theme_with(STANDARD, &tones, SEMANTICS).unwrap_err();
    assert_eq!(problems, ["tokens.json `tone-neutral-field-hover` has no dark colour"]);
}

#[test]
fn a_tone_token_with_an_empty_word_or_a_role_twice_is_refused() {
    let tones =
        tone("neutral", &["background", "field--hover", "well", "field-hover", "fieldHover"]);
    let problems = theme_with(STANDARD, &tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        [
            "tokens.json `tone-neutral-field--hover`: a tone token ends in its role, kebab-case words with no empty one",
            "tokens.json `tone-neutral-field-hover` and `tone-neutral-fieldHover` are both tone `neutral`'s role `fieldHover`",
        ]
    );
}

#[test]
fn a_tone_reads_its_roles_in_camel_case() {
    let tones = tone("neutral", &["background", "field-hover"]);
    let theme = theme_with(STANDARD, &tones, SEMANTICS).unwrap();
    let roles: Vec<&str> = theme.tones[0].surfaces.dark.keys().map(String::as_str).collect();
    assert_eq!(roles, ["background", "fieldHover"]);
}

#[test]
fn semantics_must_match_across_appearances() {
    let tones = tone("neutral", &["background"]);
    let semantics =
        "[semantics.light]\ndanger = \"#000000\"\n[semantics.dark]\nwarning = \"#000000\"\n";
    let problems = theme_with(STANDARD, &tones, semantics).unwrap_err();
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
    let tones = tone("neutral", &["background"]);
    let standard = "[standard]\ntone = \"dusk\"\naccent = \"mauve\"\n";
    let problems = theme_with(standard, &tones, SEMANTICS).unwrap_err();
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
    let theme = from_toml(THEME).unwrap();
    let tone = &theme.tones[theme.standard.tone].name;
    let accent = &theme.accents[theme.standard.accent].name;
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    assert!(swift.contains(&format!("    public static let standard = SurfaceTone.{tone}\n")));
    assert!(swift.contains(&format!("    public static let standard = AccentChoice.{accent}\n")));
    assert!(kotlin.contains(&format!("        val STANDARD = {}\n", tone.to_uppercase())));
    assert!(kotlin.contains(&format!("        val STANDARD = {}\n", accent.to_uppercase())));
}

#[test]
fn names_must_become_identifiers() {
    let tones = tone("Neutral", &["background", "Back-ground"]);
    let standard = "[standard]\ntone = \"Neutral\"\naccent = \"blue\"\n";
    let problems = theme_with(standard, &tones, SEMANTICS).unwrap_err();
    assert_eq!(
        problems,
        [
            "tone `Neutral`: a name is lowercase letters",
            "role `BackGround`: a role name is lowerCamelCase",
        ]
    );
}

#[test]
fn both_platforms_carry_every_role() {
    let theme = from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for tone in &theme.tones {
        assert!(swift.contains(&format!("    case {}\n", tone.name)));
        assert!(kotlin.contains(&format!("    {},\n", tone.name.to_uppercase())));
    }
    for role in theme.tones[0].surfaces.light.keys().chain(theme.semantics.light.keys()) {
        assert!(
            swift.contains(&format!("    let {role}: Color\n"))
                || swift.contains(&format!("    public static let {role} = Color(\n")),
            "{role}"
        );
        assert!(kotlin.contains(&format!("    val {role}: Color,\n")), "{role}");
    }
}

#[test]
fn both_platforms_carry_every_opacity() {
    let theme = from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.opacity {
        assert!(swift.contains(&format!("    public static let {role}: Double = {value:?}\n")));
        assert!(kotlin.contains(&format!("    const val {role}: Float = {value:?}f\n")));
    }
}

#[test]
fn both_platforms_carry_every_radius() {
    let theme = from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.radius {
        assert!(swift.contains(&format!("    public static let {role}: CGFloat = {value}\n")));
        assert!(kotlin.contains(&format!("    val {role} = {value}.dp\n")));
    }
}

#[test]
fn a_role_takes_each_platforms_size_from_its_style_and_shares_the_rest() {
    let theme = theme_with_text(BODY).unwrap();
    let body = &theme.text["body"];
    assert_eq!(body.weight, Weight::Regular);
    assert_eq!(
        body.apple.as_ref().map(|apple| (apple.macos, apple.ios.as_str())),
        Some((13.0, "body"))
    );
    assert_eq!((body.android, body.android_line), (Some(14.0), Some(20.0)));
    assert!(!body.monospaced);
    let mono = &from_toml(THEME).unwrap().text["mono"];
    assert!(mono.monospaced, "the Android style of mono draws in the mono family");
    assert_eq!(mono.android, Some(15.0));
}

#[test]
fn an_apple_style_names_a_dynamic_type_style() {
    let theme = theme_with_text(&BODY.replace(r#", "ios": "body""#, ""));
    assert_eq!(
        theme.unwrap_err(),
        ["tokens.json text style `body` is Apple's and names no ios style"]
    );
    let theme = theme_with_text(&BODY.replace(r#""ios": "body""#, r#""ios": "huge""#));
    assert_eq!(
        theme.unwrap_err(),
        ["tokens.json text style `body`: `huge` is not a Dynamic Type style"]
    );
}

#[test]
fn a_roles_styles_share_weight_and_letter_spacing_and_one_style_gives_each_platform() {
    let theme = theme_with_text(&BODY.replace(
        r#""fontSize": "14px", "fontWeight": 400"#,
        r#""fontSize": "14px", "fontWeight": 500"#,
    ));
    assert_eq!(
        theme.unwrap_err(),
        ["tokens.json text styles `body` and `android-body` of role `body` differ in weight, letter spacing or face; the platforms share them"]
    );
    let twice = BODY.replace(
        r#""styles": [{ "name": "body""#,
        r#""styles": [{ "name": "copy", "role": "body", "fontSize": "12px", "fontWeight": 400, "ios": "body" }, { "name": "body""#,
    );
    assert_eq!(
        theme_with_text(&twice).unwrap_err(),
        ["tokens.json `copy` and `body` both give text role `body` Apple's value"]
    );
}

#[test]
fn a_text_style_measures_in_pixels_and_draws_a_weight_the_apps_have() {
    let theme = theme_with_text(
        &BODY
            .replace(r#""13px""#, r#""13pt""#)
            .replace(r#""fontWeight": 400, "ios""#, r#""fontWeight": 450, "ios""#),
    );
    assert_eq!(
        theme.unwrap_err(),
        [
            "tokens.json text style `body`: the apps draw no weight 450",
            "tokens.json `body`: the size `13pt` is not a length in px",
        ]
    );
}

#[test]
fn both_platforms_carry_every_text_role() {
    let theme = from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, text) in &theme.text {
        assert_eq!(
            swift.contains(&format!("    public static let {role} = ThemeText(\n")),
            text.apple.is_some()
        );
        assert_eq!(kotlin.contains(&format!("        val {role} =\n")), text.android.is_some());
    }
}

/// The theme of a tokens.json with one tone, one accent, the `body` text
/// role and the icon and size families of <lengths>.
fn theme_with_lengths(lengths: &str) -> Result<Theme, Vec<String>> {
    let accent =
        ["accent-blue", "accent-blue-fill", "on-accent-blue-fill", "accent-blue-selection"]
            .map(|name| token(name, "#000000", "#000000"));
    let tokens = tokens_json(&[tone("neutral", &["background"]), accent.to_vec()].concat())
        .replace(LENGTHS, lengths);
    Theme::from_sources(&format!("{STANDARD}\n[opacity]\n{SEMANTICS}"), &tokens)
}

#[test]
fn an_icon_and_a_size_take_apples_and_androids_lengths_from_their_tokens() {
    let theme = theme_with_lengths(LENGTHS).unwrap();
    let small = &theme.icon["small"];
    let apple = small.apple.as_ref().unwrap();
    assert_eq!((apple.macos, apple.ios, apple.weight), (11.0, 13.0, Weight::Semibold));
    assert_eq!(small.android, Some(16.0));
    let transport = &theme.icon["androidTransport"];
    assert!(transport.apple.is_none());
    assert_eq!(transport.android, Some(36.0));
    let hit = &theme.size["hitTarget"];
    assert_eq!((hit.macos, hit.ios, hit.android), (28.0, 44.0, 44.0));
    assert_eq!(theme.size.len(), 1, "a token with no role is no app role");
}

#[test]
fn a_size_needs_a_token_on_each_platform_and_an_apple_icon_its_weight() {
    let no_weight = LENGTHS.replace(r#", "weight": "semibold""#, "");
    assert_eq!(
        theme_with_lengths(&no_weight).unwrap_err(),
        ["tokens.json `icon-small` names no weight"]
    );
    let no_ios = LENGTHS.replace(r#", "ios": "44px""#, "");
    assert_eq!(
        theme_with_lengths(&no_ios).unwrap_err(),
        ["tokens.json `size-hit-target` names no ios length"]
    );
    let no_role = LENGTHS.replace(r#""role": "hitTarget", "value": "44px""#, r#""value": "44px""#);
    assert_eq!(
        theme_with_lengths(&no_role).unwrap_err(),
        [
            "tokens.json `size-android-hit-target` names no role",
            "size role `hitTarget` has no Android token in tokens.json",
        ]
    );
}

#[test]
fn both_platforms_carry_every_space_icon_and_size() {
    let theme = from_toml(THEME).unwrap();
    let swift = apple::swift(&theme);
    let kotlin = android::kotlin(&theme);
    for (role, value) in &theme.space {
        assert!(swift.contains(&format!("    public static let {role}: CGFloat = {value}\n")));
        assert!(kotlin.contains(&format!("    val {role} = {value}.dp\n")));
    }
    for (role, icon) in &theme.icon {
        assert_eq!(
            swift.contains(&format!("    public static let {role} = ThemeIcon(\n")),
            icon.apple.is_some()
        );
        match icon.android {
            Some(size) => assert!(kotlin.contains(&format!("    val {role} = {size}.dp\n"))),
            None => assert!(!kotlin.contains(&format!("    val {role} = "))),
        }
    }
    for (role, lengths) in &theme.size {
        assert!(swift.contains(&format!("    public static let {role}: CGFloat = .platform(\n")));
        assert!(kotlin.contains(&format!("    val {role} = {}.dp\n", lengths.android)));
    }
}

#[test]
fn the_launch_background_is_the_standard_tones() {
    let theme = from_toml(THEME).unwrap();
    let resources = android::launch_resources(&theme).unwrap();
    // Plum's backgrounds.
    assert_eq!(resources[0].0, "res/values/theme_colors.xml");
    assert!(resources[0].1.contains(">#FFEFE4F1<"));
    assert_eq!(resources[1].0, "res/values-night/theme_colors.xml");
    assert!(resources[1].1.contains(">#FF201B25<"));
}

#[test]
fn a_text_role_with_an_android_line_height_carries_it_into_kotlin() {
    let theme = theme_with_text(&BODY.replace(r#""20px""#, r#""22px""#)).unwrap();
    let kotlin = android::kotlin(&theme);
    assert!(kotlin.contains("fontSize = 14.sp,\n                    lineHeight = 22.sp,\n"));
}

/// The Mac app's AccentColor asset is the standard accent's selection colour,
/// which macOS draws system parts in while the person's system accent is
/// Multicolor. The asset is a catalog file Xcode compiles, so theme-gen
/// cannot write it; this holds it to the theme.
#[test]
fn the_mac_apps_accent_color_asset_is_the_standard_accents_selection() {
    const ASSET: &str =
        include_str!("../../bae-macos/bae/bae/Assets.xcassets/AccentColor.colorset/Contents.json");
    let theme = from_toml(THEME).unwrap();
    let Argb(selection) = theme.accents[theme.standard.accent].selection;
    let asset: serde_json::Value = serde_json::from_str(ASSET).unwrap();
    let components = &asset["colors"][0]["color"]["components"];
    let channel = |name: &str| {
        let hex = components[name].as_str().unwrap().trim_start_matches("0x");
        u32::from_str_radix(hex, 16).unwrap()
    };
    let drawn = channel("red") << 16 | channel("green") << 8 | channel("blue");
    assert_eq!(drawn, selection & 0x00FF_FFFF);
    assert_eq!(components["alpha"], "1.000");
}
