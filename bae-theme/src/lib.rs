//! The apps' shared theme: `design/theme.toml`, validated, and emitted as the
//! typed Swift and Kotlin each platform compiles.

pub mod android;
pub mod apple;

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
        let value =
            u32::from_str_radix(digits, 16).map_err(|_| format!("`{hex}` is not a colour"))?;
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

/// A background tone the person can choose.
#[derive(Debug)]
pub struct Tone {
    pub name: String,
    pub surfaces: Modes,
}

/// An accent the person can choose: text and glyph colours per appearance,
/// and the fill behind white button labels.
#[derive(Debug)]
pub struct Accent {
    pub name: String,
    pub light: Argb,
    pub dark: Argb,
    pub fill: Argb,
}

/// The validated theme. Every tone has the same surface roles, and the
/// semantic colours have the same roles in both appearances.
#[derive(Debug)]
pub struct Theme {
    pub tones: Vec<Tone>,
    pub accents: Vec<Accent>,
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

/// A length each platform sets by its own convention: points on macOS and
/// iOS, dp on Android.
#[derive(Debug, Clone, Copy)]
pub struct PlatformLengths {
    pub macos: f64,
    pub ios: f64,
    pub android: f64,
}

/// A glyph size, and on Apple the weight SF Symbols draw it at.
#[derive(Debug)]
pub struct IconRole {
    pub weight: Weight,
    pub sizes: PlatformLengths,
}

/// A text role: weight, tracking, case and monospacing shared by every
/// platform, and a size for each.
#[derive(Debug)]
pub struct TextRole {
    pub weight: Weight,
    /// Letter spacing, in points and sp.
    pub tracking: f64,
    pub uppercase: bool,
    pub monospaced: bool,
    /// Points.
    pub macos: f64,
    /// A Dynamic Type style, so iOS text follows the person's text size.
    pub ios: String,
    /// sp.
    pub android: f64,
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
    tones: Vec<RawTone>,
    accents: Vec<RawAccent>,
    semantics: RawModes,
    opacity: BTreeMap<String, f64>,
    radius: BTreeMap<String, f64>,
    text: BTreeMap<String, RawTextRole>,
    space: BTreeMap<String, f64>,
    icon: BTreeMap<String, RawIconRole>,
    size: BTreeMap<String, RawPlatformLengths>,
}

/// Lengths as written; a platform left out is reported rather than defaulted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlatformLengths {
    macos: Option<f64>,
    ios: Option<f64>,
    android: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawIconRole {
    weight: Weight,
    macos: Option<f64>,
    ios: Option<f64>,
    android: Option<f64>,
}

impl RawPlatformLengths {
    /// Every platform's length, or a problem naming each one missing.
    fn check(&self, context: &str, problems: &mut Vec<String>) -> PlatformLengths {
        let mut length = |platform: &str, value: Option<f64>| match value {
            Some(value) if value >= 0.0 => value,
            Some(value) => {
                problems.push(format!(
                    "{context}: the {platform} length {value} is negative"
                ));
                0.0
            }
            None => {
                problems.push(format!("{context} has no {platform} length"));
                0.0
            }
        };
        PlatformLengths {
            macos: length("macos", self.macos),
            ios: length("ios", self.ios),
            android: length("android", self.android),
        }
    }
}

/// A text role as written; a platform left out is reported rather than
/// defaulted.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTextRole {
    weight: Weight,
    tracking: Option<f64>,
    uppercase: Option<bool>,
    monospaced: Option<bool>,
    macos: Option<f64>,
    ios: Option<String>,
    android: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModes {
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTone {
    name: String,
    light: BTreeMap<String, String>,
    dark: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAccent {
    name: String,
    light: String,
    dark: String,
    fill: String,
}

impl Theme {
    /// Parses and validates `design/theme.toml`, naming every problem found.
    pub fn from_toml(source: &str) -> Result<Self, Vec<String>> {
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
        let tones: Vec<Tone> = raw
            .tones
            .iter()
            .map(|tone| Tone {
                name: tone.name.clone(),
                surfaces: Modes {
                    light: roles(&format!("tone {}.light", tone.name), &tone.light),
                    dark: roles(&format!("tone {}.dark", tone.name), &tone.dark),
                },
            })
            .collect();
        let semantics = Modes {
            light: roles("semantics.light", &raw.semantics.light),
            dark: roles("semantics.dark", &raw.semantics.dark),
        };
        let accents: Vec<Accent> = raw
            .accents
            .iter()
            .map(|accent| {
                let context = format!("accent {}", accent.name);
                Accent {
                    name: accent.name.clone(),
                    light: colour(&format!("{context}.light"), &accent.light),
                    dark: colour(&format!("{context}.dark"), &accent.dark),
                    fill: colour(&format!("{context}.fill"), &accent.fill),
                }
            })
            .collect();

        check_choices(
            "tone",
            tones.iter().map(|tone| tone.name.as_str()),
            &mut problems,
        );
        check_choices(
            "accent",
            accents.iter().map(|accent| accent.name.as_str()),
            &mut problems,
        );
        if let Some(first) = tones.first() {
            for tone in &tones {
                for (mode, surfaces) in [
                    ("light", &tone.surfaces.light),
                    ("dark", &tone.surfaces.dark),
                ] {
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
        for (name, value) in &raw.radius {
            if *value < 0.0 {
                problems.push(format!("radius {name}: {value} is negative"));
            }
        }
        check_role_names(raw.radius.keys(), &mut problems);
        check_role_names(raw.text.keys(), &mut problems);
        for (name, value) in &raw.space {
            if *value < 0.0 {
                problems.push(format!("space {name}: {value} is negative"));
            }
        }
        check_role_names(raw.space.keys(), &mut problems);
        check_role_names(raw.icon.keys(), &mut problems);
        let icon = raw
            .icon
            .iter()
            .map(|(name, role)| {
                let lengths = RawPlatformLengths {
                    macos: role.macos,
                    ios: role.ios,
                    android: role.android,
                };
                let sizes = lengths.check(&format!("icon {name}"), &mut problems);
                (
                    name.clone(),
                    IconRole {
                        weight: role.weight,
                        sizes,
                    },
                )
            })
            .collect();
        check_role_names(raw.size.keys(), &mut problems);
        let size = raw
            .size
            .iter()
            .map(|(name, lengths)| {
                (
                    name.clone(),
                    lengths.check(&format!("size {name}"), &mut problems),
                )
            })
            .collect();
        let text = raw
            .text
            .iter()
            .map(|(name, role)| {
                let mut lacks =
                    |platform: &str| problems.push(format!("text {name} has no {platform} size"));
                let macos = role.macos.unwrap_or_else(|| {
                    lacks("macos");
                    0.0
                });
                let ios = role.ios.clone().unwrap_or_else(|| {
                    lacks("ios");
                    String::new()
                });
                let android = role.android.unwrap_or_else(|| {
                    lacks("android");
                    0.0
                });
                if !ios.is_empty() && !IOS_TEXT_STYLES.contains(&ios.as_str()) {
                    problems.push(format!("text {name}: `{ios}` is not a Dynamic Type style"));
                }
                (
                    name.clone(),
                    TextRole {
                        weight: role.weight,
                        tracking: role.tracking.unwrap_or(0.0),
                        uppercase: role.uppercase.unwrap_or(false),
                        monospaced: role.monospaced.unwrap_or(false),
                        macos,
                        ios,
                        android,
                    },
                )
            })
            .collect();

        if problems.is_empty() {
            Ok(Self {
                tones,
                accents,
                semantics,
                opacity: raw.opacity,
                radius: raw.radius,
                text,
                space: raw.space,
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

/// A role name becomes a property on both platforms.
fn check_role_names<'a>(names: impl Iterator<Item = &'a String>, problems: &mut Vec<String>) {
    for name in names {
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
        problems.push(format!(
            "{context} lacks `{name}`, which {reference_context} has"
        ));
    }
    for name in roles.keys().filter(|name| !reference.contains_key(*name)) {
        problems.push(format!(
            "{context} has `{name}`, which {reference_context} lacks"
        ));
    }
}

#[cfg(test)]
mod tests;
