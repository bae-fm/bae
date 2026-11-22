//! The apps' shared theme: `design/theme.toml` for the semantics and scales
//! and `design/system/tokens.json` for the tones, the accents, the text
//! styles, the radii, the spacing and the icon and size roles, validated, and emitted as the typed Swift and Kotlin each platform
//! compiles.

pub mod android;
pub mod apple;
mod lengths;

use std::collections::BTreeMap;

use serde::Deserialize;

/// A colour as its alpha, red, green and blue bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argb(pub u32);

impl Argb {
    /// Reads `#RRGGBB`, which is opaque, or `#AARRGGBB`.
    fn parse(hex: &str) -> Result<Self, String> {
        let digits = hex
            .strip_prefix('#')
            .filter(|digits| digits.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| format!("`{hex}` is not a #RRGGBB or #AARRGGBB colour"))?;
        let value = u32::from_str_radix(digits, 16)
            .map_err(|error| format!("`{hex}` is not a colour: {error}"))?;
        match digits.len() {
            6 => Ok(Self(0xFF00_0000 | value)),
            8 => Ok(Self(value)),
            _ => Err(format!("`{hex}` is not a #RRGGBB or #AARRGGBB colour")),
        }
    }
}

/// Named colours, in name order.
pub type Roles = BTreeMap<String, Argb>;

/// The same roles in light and dark appearance.
#[derive(Debug)]
pub struct Modes {
    pub light: Roles,
    pub dark: Roles,
}

/// A background tone the person can choose, as design/system/tokens.json
/// defines it: one `tone-<name>-<role>` token per surface role, with a colour
/// per appearance. The role is the token's kebab-case suffix in lowerCamelCase.
#[derive(Debug)]
pub struct Tone {
    pub name: String,
    pub surfaces: Modes,
}

/// An accent the person can choose, as design/system/tokens.json defines it:
/// text and glyph colours per appearance (`accent-<name>`), its swatch in the
/// picker (`accent-<name>-fill`) with the colour of what is drawn on the
/// swatch (`on-accent-<name>-fill`), and the fill behind white labels on
/// primary buttons and the chosen pane (`accent-<name>-selection`). The
/// swatch, its content and the selection are one colour in both appearances.
#[derive(Debug)]
pub struct Accent {
    pub name: String,
    pub light: Argb,
    pub dark: Argb,
    pub fill: Argb,
    pub on_fill: Argb,
    pub selection: Argb,
}

impl Accent {
    /// The accent's colours by role name.
    pub fn roles(&self) -> Roles {
        [
            ("light", self.light),
            ("dark", self.dark),
            ("fill", self.fill),
            ("onFill", self.on_fill),
            ("selection", self.selection),
        ]
        .into_iter()
        .map(|(name, colour)| (name.to_string(), colour))
        .collect()
    }
}

/// The validated theme. Every tone has the same surface roles, and the
/// semantic colours have the same roles in both appearances.
#[derive(Debug)]
pub struct Theme {
    pub tones: Vec<Tone>,
    pub accents: Vec<Accent>,
    pub standard: Standard,
    pub semantics: Modes,
    /// How opaque a colour is laid over what is behind it, by role.
    pub opacity: BTreeMap<String, f64>,
    /// Corner radii, in points and dp, by role.
    pub radius: BTreeMap<String, f64>,
    /// Text styles, by role.
    pub text: BTreeMap<String, TextRole>,
    /// Gaps and insets, in points and dp, by role.
    pub space: BTreeMap<String, f64>,
    /// Glyph sizes, by role.
    pub icon: BTreeMap<String, IconRole>,
    /// Recurring element sizes, by role.
    pub size: BTreeMap<String, PlatformLengths>,
}

/// The tone and accent a new installation shows until the person chooses
/// one, as positions in `Theme::tones` and `Theme::accents`.
#[derive(Debug)]
pub struct Standard {
    pub tone: usize,
    pub accent: usize,
}

/// A length each platform sets by its own convention: points on macOS and
/// iOS, dp on Android.
#[derive(Debug, Clone, Copy)]
pub struct PlatformLengths {
    pub macos: f64,
    pub ios: f64,
    pub android: f64,
}

/// A glyph size on each platform that draws the role. Apple's code takes
/// macOS and iOS together; a platform whose code reads a role it lacks fails
/// to compile.
#[derive(Debug)]
pub struct IconRole {
    pub apple: Option<AppleIcon>,
    /// dp.
    pub android: Option<f64>,
}

/// What macOS and iOS draw a glyph at, in points, and the weight SF Symbols
/// draw it at.
#[derive(Debug)]
pub struct AppleIcon {
    pub macos: f64,
    pub ios: f64,
    pub weight: Weight,
}

/// A text role: weight, tracking and monospacing shared by every
/// platform, and a size for each platform that uses it. Apple's code takes
/// macOS and iOS together; a platform whose code reads a role it lacks fails
/// to compile. tokens.json gives a role one text style per platform that
/// draws it, each naming the role in its `role`.
#[derive(Debug)]
pub struct TextRole {
    pub weight: Weight,
    /// Letter spacing, in points and sp.
    pub tracking: f64,
    pub monospaced: bool,
    pub apple: Option<AppleSize>,
    /// sp.
    pub android: Option<f64>,
    /// Android's line height in sp, for a role whose lines are not the font's
    /// own height.
    pub android_line: Option<f64>,
}

/// What macOS and iOS draw a text role at.
#[derive(Debug)]
pub struct AppleSize {
    /// Points.
    pub macos: f64,
    /// A Dynamic Type style, so iOS text follows the person's text size.
    pub ios: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weight {
    Thin,
    Regular,
    Medium,
    Semibold,
    Bold,
    Heavy,
}

impl Weight {
    pub fn swift(self) -> &'static str {
        match self {
            Self::Thin => ".thin",
            Self::Regular => ".regular",
            Self::Medium => ".medium",
            Self::Semibold => ".semibold",
            Self::Bold => ".bold",
            Self::Heavy => ".heavy",
        }
    }

    pub fn kotlin(self) -> &'static str {
        match self {
            Self::Thin => "FontWeight.Thin",
            Self::Regular => "FontWeight.Normal",
            Self::Medium => "FontWeight.Medium",
            Self::Semibold => "FontWeight.SemiBold",
            Self::Bold => "FontWeight.Bold",
            Self::Heavy => "FontWeight.ExtraBold",
        }
    }
}

/// The Dynamic Type styles an iOS size can name.
const IOS_TEXT_STYLES: &[&str] = &[
    "largeTitle",
    "title",
    "title2",
    "title3",
    "headline",
    "subheadline",
    "body",
    "callout",
    "footnote",
    "caption",
    "caption2",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    standard: RawStandard,
    semantics: RawModes,
    opacity: BTreeMap<String, f64>,
}

/// The standard tone and accent by name.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStandard {
    tone: String,
    accent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModes {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
}

/// design/system/tokens.json, of which the tones, the accents, the text
/// styles, the radii, the spacing and the icon and size roles are read.
#[derive(Deserialize)]
struct RawTokens {
    color: RawColorTokens,
    #[serde(rename = "type")]
    text: RawType,
    radius: lengths::RawLengthFamily,
    spacing: lengths::RawLengthFamily,
    icon: lengths::RawLengthFamily,
    #[serde(rename = "icon-android")]
    icon_android: lengths::RawLengthFamily,
    size: lengths::RawLengthFamily,
    #[serde(rename = "size-android")]
    size_android: lengths::RawLengthFamily,
}

#[derive(Deserialize)]
struct RawType {
    groups: Vec<RawTypeGroup>,
}

/// A group of text styles in one font family. The `android` family's styles
/// are Android's sizes; every other family's are Apple's.
#[derive(Deserialize)]
struct RawTypeGroup {
    family: String,
    styles: Vec<RawTextStyle>,
}

/// A text style as tokens.json writes it for the design system's stylesheet,
/// with the app role it draws and, on Apple, the Dynamic Type style iOS
/// follows.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTextStyle {
    name: String,
    role: String,
    family: Option<String>,
    font_size: String,
    font_weight: u16,
    line_height: Option<String>,
    letter_spacing: Option<String>,
    ios: Option<String>,
}

/// The family whose text styles are Android's sizes.
const ANDROID_FAMILY: &str = "android";
/// The family whose text styles draw in the platform's monospaced face.
const MONO_FAMILY: &str = "mono";

#[derive(Deserialize)]
struct RawColorTokens {
    tokens: Vec<RawToken>,
}

#[derive(Deserialize)]
struct RawToken {
    name: String,
    value: serde_json::Value,
}

/// A token's colour in each appearance, as (light, dark), or a problem for
/// each appearance it lacks or whose colour does not parse.
fn appearances(
    by_name: &BTreeMap<&str, &serde_json::Value>,
    token: &str,
    owner: &str,
    problems: &mut Vec<String>,
) -> Option<(Argb, Argb)> {
    let Some(value) = by_name.get(token) else {
        problems.push(format!("tokens.json has no `{token}`, which {owner} needs"));
        return None;
    };
    let mut read = |mode: &str| {
        let hex = value.get(mode).and_then(serde_json::Value::as_str);
        match hex.map(Argb::parse) {
            Some(Ok(colour)) => Some(colour),
            Some(Err(problem)) => {
                problems.push(format!("tokens.json `{token}`.{mode}: {problem}"));
                None
            }
            None => {
                problems.push(format!("tokens.json `{token}` has no {mode} colour"));
                None
            }
        }
    };
    let light = read("light");
    let dark = read("dark");
    Some((light?, dark?))
}

/// What tokens.json defines for the apps: the tones and the accents, each in
/// its order, and the text, radius, spacing, icon and size roles.
struct TokenRoles {
    tones: Vec<Tone>,
    accents: Vec<Accent>,
    text: BTreeMap<String, TextRole>,
    radius: BTreeMap<String, f64>,
    space: BTreeMap<String, f64>,
    icon: BTreeMap<String, IconRole>,
    size: BTreeMap<String, PlatformLengths>,
}

fn read_tokens(tokens: &str, problems: &mut Vec<String>) -> TokenRoles {
    let raw: RawTokens = match serde_json::from_str(tokens) {
        Ok(raw) => raw,
        Err(error) => {
            problems.push(format!("tokens.json: {error}"));
            return TokenRoles {
                tones: Vec::new(),
                accents: Vec::new(),
                text: BTreeMap::new(),
                radius: BTreeMap::new(),
                space: BTreeMap::new(),
                icon: BTreeMap::new(),
                size: BTreeMap::new(),
            };
        }
    };
    let by_name: BTreeMap<&str, &serde_json::Value> =
        raw.color.tokens.iter().map(|token| (token.name.as_str(), &token.value)).collect();
    let names: Vec<&str> = raw.color.tokens.iter().map(|token| token.name.as_str()).collect();
    TokenRoles {
        tones: read_tones(&names, &by_name, problems),
        accents: read_accents(&names, &by_name, problems),
        text: read_text(&raw.text, problems),
        radius: lengths::read_shared("radius", &raw.radius, problems),
        space: lengths::read_shared("space", &raw.spacing, problems),
        icon: lengths::read_icons(&raw.icon, &raw.icon_android, problems),
        size: lengths::read_sizes(&raw.size, &raw.size_android, problems),
    }
}

/// A length tokens.json writes in CSS pixels (`13px`) for its token or text
/// style <name>, as points, sp and dp.
fn pixels(name: &str, what: &str, css: &str, problems: &mut Vec<String>) -> f64 {
    match css.strip_suffix("px").map(str::parse::<f64>) {
        Some(Ok(value)) => value,
        _ => {
            problems
                .push(format!("tokens.json `{name}`: the {what} `{css}` is not a length in px"));
            0.0
        }
    }
}

/// A CSS font weight as the weight the apps draw.
fn css_weight(style: &str, weight: u16, problems: &mut Vec<String>) -> Weight {
    match weight {
        400 => Weight::Regular,
        500 => Weight::Medium,
        600 => Weight::Semibold,
        700 => Weight::Bold,
        800 => Weight::Heavy,
        other => {
            problems
                .push(format!("tokens.json text style `{style}`: the apps draw no weight {other}"));
            Weight::Regular
        }
    }
}

/// One platform's text style of a role, read.
struct PlatformStyle<'a> {
    name: &'a str,
    weight: Weight,
    tracking: f64,
    monospaced: bool,
    size: f64,
    line: Option<f64>,
    ios: Option<&'a String>,
}

/// The text roles of tokens.json's text styles: each role's weight, tracking
/// and monospacing, which its styles share, Apple's size from its style in an
/// Apple family and Android's from its style in the `android` family.
fn read_text(raw: &RawType, problems: &mut Vec<String>) -> BTreeMap<String, TextRole> {
    let mut entries: Vec<RoleEntry<PlatformStyle>> = Vec::new();
    for group in &raw.groups {
        for style in &group.styles {
            let name = style.name.as_str();
            let read = PlatformStyle {
                name,
                weight: css_weight(name, style.font_weight, problems),
                tracking: style
                    .letter_spacing
                    .as_ref()
                    .map_or(0.0, |spacing| pixels(name, "letter spacing", spacing, problems)),
                monospaced: style.family.as_ref().unwrap_or(&group.family) == MONO_FAMILY,
                size: pixels(name, "size", &style.font_size, problems),
                line: style
                    .line_height
                    .as_ref()
                    .map(|line| pixels(name, "line height", line, problems)),
                ios: style.ios.as_ref(),
            };
            entries.push(RoleEntry {
                role: &style.role,
                name,
                android: group.family == ANDROID_FAMILY,
                value: read,
            });
        }
    }
    pair_roles("text", entries, problems)
        .into_iter()
        .filter_map(|(role, pair)| {
            let (apple, android) = (pair.apple, pair.android);
            let shared = match (&apple, &android) {
                (Some(apple), Some(android)) => {
                    if (apple.weight, apple.tracking, apple.monospaced)
                        != (android.weight, android.tracking, android.monospaced)
                    {
                        problems.push(format!(
                            "tokens.json text styles `{}` and `{}` of role `{role}` differ in weight, letter spacing or face; the platforms share them",
                            apple.name, android.name
                        ));
                    }
                    apple
                }
                (Some(only), None) | (None, Some(only)) => only,
                (None, None) => unreachable!("a role is read from one of its styles"),
            };
            let (weight, tracking, monospaced) = (shared.weight, shared.tracking, shared.monospaced);
            if let Some(android) = &android {
                if android.ios.is_some() {
                    problems.push(format!(
                        "tokens.json text style `{}` is Android's and names an ios style",
                        android.name
                    ));
                }
            }
            let apple = match apple {
                Some(style) => match style.ios {
                    Some(ios) => {
                        if !IOS_TEXT_STYLES.contains(&ios.as_str()) {
                            problems.push(format!(
                                "tokens.json text style `{}`: `{ios}` is not a Dynamic Type style",
                                style.name
                            ));
                        }
                        Some(AppleSize { macos: style.size, ios: ios.clone() })
                    }
                    None => {
                        problems.push(format!(
                            "tokens.json text style `{}` is Apple's and names no ios style",
                            style.name
                        ));
                        return None;
                    }
                },
                None => None,
            };
            Some((
                role.to_string(),
                TextRole {
                    weight,
                    tracking,
                    monospaced,
                    apple,
                    android: android.as_ref().map(|style| style.size),
                    android_line: android.and_then(|style| style.line),
                },
            ))
        })
        .collect()
}

/// Every `<name>` with a `tone-<name>-background` token, with each
/// `tone-<name>-<role>` token as a surface role.
fn read_tones(
    names: &[&str],
    by_name: &BTreeMap<&str, &serde_json::Value>,
    problems: &mut Vec<String>,
) -> Vec<Tone> {
    let tones = names.iter().filter_map(|token| {
        let name = token.strip_prefix("tone-")?.strip_suffix("-background")?;
        (!name.is_empty()).then_some(name)
    });
    let mut out = Vec::new();
    for name in tones {
        let prefix = format!("tone-{name}-");
        let mut surfaces = Modes { light: Roles::new(), dark: Roles::new() };
        let mut tokens_of: BTreeMap<String, &str> = BTreeMap::new();
        for token in names.iter().filter(|token| token.starts_with(&prefix)) {
            let Some(role) = camel_case(&token[prefix.len()..]) else {
                problems.push(format!(
                    "tokens.json `{token}`: a tone token ends in its role, kebab-case words with no empty one"
                ));
                continue;
            };
            if let Some(other) = tokens_of.insert(role.clone(), token) {
                problems.push(format!(
                    "tokens.json `{other}` and `{token}` are both tone `{name}`'s role `{role}`"
                ));
                continue;
            }
            let owner = format!("tone `{name}`");
            if let Some((light, dark)) = appearances(by_name, token, &owner, problems) {
                surfaces.light.insert(role.clone(), light);
                surfaces.dark.insert(role, dark);
            }
        }
        out.push(Tone { name: name.to_string(), surfaces });
    }
    out
}

/// `field-hover` as `fieldHover`, or none when a word of <kebab> is empty.
fn camel_case(kebab: &str) -> Option<String> {
    let mut out = String::new();
    for (at, part) in kebab.split('-').enumerate() {
        let mut chars = part.chars();
        let first = chars.next()?;
        out.push(if at == 0 { first } else { first.to_ascii_uppercase() });
        out.push_str(chars.as_str());
    }
    Some(out)
}

/// Every `<name>` with an `accent-<name>-fill` token. A problem is reported
/// for each accent missing one of its four tokens or whose swatch, its
/// content or selection differs between appearances.
fn read_accents(
    names: &[&str],
    by_name: &BTreeMap<&str, &serde_json::Value>,
    problems: &mut Vec<String>,
) -> Vec<Accent> {
    let accents = names.iter().filter_map(|token| {
        let name = token.strip_prefix("accent-")?.strip_suffix("-fill")?;
        (!name.is_empty()).then_some(name)
    });
    let mut out = Vec::new();
    for name in accents {
        let owner = format!("accent `{name}`");
        let mut read = |token: String| appearances(by_name, &token, &owner, problems);
        let text = read(format!("accent-{name}"));
        let fill = read(format!("accent-{name}-fill"));
        let on_fill = read(format!("on-accent-{name}-fill"));
        let selection = read(format!("accent-{name}-selection"));
        let (Some(text), Some(fill), Some(on_fill), Some(selection)) =
            (text, fill, on_fill, selection)
        else {
            continue;
        };
        for (token, (light, dark)) in [
            (format!("accent-{name}-fill"), fill),
            (format!("on-accent-{name}-fill"), on_fill),
            (format!("accent-{name}-selection"), selection),
        ] {
            if light != dark {
                problems.push(format!(
                    "tokens.json `{token}` differs between appearances; the apps draw it as one colour"
                ));
            }
        }
        out.push(Accent {
            name: name.to_string(),
            light: text.0,
            dark: text.1,
            fill: fill.0,
            on_fill: on_fill.0,
            selection: selection.0,
        });
    }
    out
}

impl Theme {
    /// Parses and validates `design/theme.toml` and the tones and accents in
    /// `design/system/tokens.json`, naming every problem found.
    pub fn from_sources(source: &str, tokens: &str) -> Result<Self, Vec<String>> {
        let raw: RawTheme = toml::from_str(source).map_err(|error| vec![error.to_string()])?;
        let mut problems = Vec::new();
        let mut colour = |context: &str, hex: &str| {
            Argb::parse(hex).unwrap_or_else(|problem| {
                problems.push(format!("{context}: {problem}"));
                Argb(0)
            })
        };
        let mut roles = |context: &str, raw: &BTreeMap<String, String>| -> Roles {
            raw.iter()
                .map(|(name, hex)| (name.clone(), colour(&format!("{context}.{name}"), hex)))
                .collect()
        };
        let semantics = Modes {
            light: roles("semantics.light", &raw.semantics.light),
            dark: roles("semantics.dark", &raw.semantics.dark),
        };
        let TokenRoles { tones, accents, text, radius, space, icon, size } =
            read_tokens(tokens, &mut problems);

        check_choices("tone", tones.iter().map(|tone| tone.name.as_str()), &mut problems);
        check_choices("accent", accents.iter().map(|accent| accent.name.as_str()), &mut problems);
        let standard_tone = standard_choice(
            "tone",
            &raw.standard.tone,
            tones.iter().map(|tone| tone.name.as_str()),
            &mut problems,
        );
        let standard_accent = standard_choice(
            "accent",
            &raw.standard.accent,
            accents.iter().map(|accent| accent.name.as_str()),
            &mut problems,
        );
        if let Some(first) = tones.first() {
            for tone in &tones {
                for (mode, surfaces) in
                    [("light", &tone.surfaces.light), ("dark", &tone.surfaces.dark)]
                {
                    check_same_roles(
                        &format!("tone {}.{mode}", tone.name),
                        surfaces,
                        &format!("tone {}.light", first.name),
                        &first.surfaces.light,
                        &mut problems,
                    );
                }
            }
            check_role_names(first.surfaces.light.keys(), &mut problems);
        }
        check_same_roles(
            "semantics.dark",
            &semantics.dark,
            "semantics.light",
            &semantics.light,
            &mut problems,
        );
        check_role_names(semantics.light.keys(), &mut problems);
        for (name, value) in &raw.opacity {
            if !(0.0..=1.0).contains(value) {
                problems.push(format!("opacity {name}: {value} is not between 0 and 1"));
            }
        }
        check_role_names(raw.opacity.keys(), &mut problems);

        if let (true, Some(tone), Some(accent)) =
            (problems.is_empty(), standard_tone, standard_accent)
        {
            Ok(Self {
                tones,
                accents,
                standard: Standard { tone, accent },
                semantics,
                opacity: raw.opacity,
                radius,
                text,
                space,
                icon,
                size,
            })
        } else {
            Err(problems)
        }
    }
}

/// A tone or accent name becomes a Swift case and a Kotlin enum entry.
fn check_choices<'a>(kind: &str, names: impl Iterator<Item = &'a str>, problems: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    let mut any = false;
    for name in names {
        any = true;
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase()) {
            problems.push(format!("{kind} `{name}`: a name is lowercase letters"));
        }
        if !seen.insert(name) {
            problems.push(format!("{kind} `{name}` is declared twice"));
        }
    }
    if !any {
        problems.push(format!("there is no {kind}"));
    }
}

/// The position of the standard `kind` among those listed, or a problem when
/// it names none of them.
fn standard_choice<'a>(
    kind: &str,
    name: &str,
    mut names: impl Iterator<Item = &'a str>,
    problems: &mut Vec<String>,
) -> Option<usize> {
    let position = names.position(|listed| listed == name);
    if position.is_none() {
        problems.push(format!("the standard {kind} `{name}` is not a listed {kind}"));
    }
    position
}

/// A role's entry on Apple and its entry on Android, either of which may be
/// missing.
pub(crate) struct RolePair<T> {
    apple: Option<T>,
    android: Option<T>,
}

/// One entry tokens.json gives a <kind> role: the token or text style named
/// <name> gives the role's value on Android when <android> is true, else on
/// Apple.
pub(crate) struct RoleEntry<'a, T> {
    pub(crate) role: &'a str,
    pub(crate) name: &'a str,
    pub(crate) android: bool,
    pub(crate) value: T,
}

/// Each role's Apple and Android entries. Two entries that give one role the
/// same platform's value are reported, the first kept, and every role name is
/// checked.
pub(crate) fn pair_roles<'a, T>(
    kind: &str,
    entries: impl IntoIterator<Item = RoleEntry<'a, T>>,
    problems: &mut Vec<String>,
) -> BTreeMap<&'a str, RolePair<T>> {
    let mut roles: BTreeMap<&str, RolePair<RoleEntry<T>>> = BTreeMap::new();
    for entry in entries {
        let pair = roles.entry(entry.role).or_insert(RolePair { apple: None, android: None });
        let (slot, platform) =
            if entry.android { (&mut pair.android, "Android") } else { (&mut pair.apple, "Apple") };
        match slot {
            Some(first) => problems.push(format!(
                "tokens.json `{}` and `{}` both give {kind} role `{}` {platform}'s value",
                first.name, entry.name, entry.role
            )),
            None => *slot = Some(entry),
        }
    }
    let roles: BTreeMap<&str, RolePair<T>> = roles
        .into_iter()
        .map(|(role, pair)| {
            let value = |entry: RoleEntry<'a, T>| entry.value;
            (role, RolePair { apple: pair.apple.map(value), android: pair.android.map(value) })
        })
        .collect();
    check_role_names(roles.keys(), problems);
    roles
}

/// A role name becomes a property on both platforms.
fn check_role_names(names: impl Iterator<Item = impl AsRef<str>>, problems: &mut Vec<String>) {
    for name in names {
        let name = name.as_ref();
        let mut chars = name.chars();
        let starts_lower = chars.next().is_some_and(|c| c.is_ascii_lowercase());
        if !starts_lower || !chars.all(|c| c.is_ascii_alphanumeric()) {
            problems.push(format!("role `{name}`: a role name is lowerCamelCase"));
        }
    }
}

fn check_same_roles(
    context: &str,
    roles: &Roles,
    reference_context: &str,
    reference: &Roles,
    problems: &mut Vec<String>,
) {
    for name in reference.keys().filter(|name| !roles.contains_key(*name)) {
        problems.push(format!("{context} lacks `{name}`, which {reference_context} has"));
    }
    for name in roles.keys().filter(|name| !reference.contains_key(*name)) {
        problems.push(format!("{context} has `{name}`, which {reference_context} lacks"));
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
