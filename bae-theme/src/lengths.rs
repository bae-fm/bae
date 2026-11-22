//! The icon and size roles of design/system/tokens.json. A role is a token in
//! an Apple family (`icon`, `size`), whose value is the macOS length and whose
//! `ios` is iOS's, and a token in its Android family (`icon-android`,
//! `size-android`), whose value is Android's; each names the role in `role`.
//! An icon role may be one platform's alone, as a text role may; a size role
//! has both. An Apple token with no `role` is a design-system measure no app
//! draws as a role, such as a macOS control's height.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{check_role_names, pixels, AppleIcon, IconRole, PlatformLengths, Weight};

/// A family of length tokens.
#[derive(Deserialize)]
pub(crate) struct RawLengthFamily {
    tokens: Vec<RawLengthToken>,
}

#[derive(Deserialize)]
struct RawLengthToken {
    name: String,
    role: Option<String>,
    value: String,
    ios: Option<String>,
    /// On Apple, the weight SF Symbols draw an icon at.
    weight: Option<Weight>,
}

/// A role's Apple token and Android token, either of which may be missing.
struct RoleTokens<'a> {
    apple: Option<&'a RawLengthToken>,
    android: Option<&'a RawLengthToken>,
}

/// The tokens of an Apple and an Android family of <kind> tokens, by role.
fn read_roles<'a>(
    kind: &str,
    apple: &'a RawLengthFamily,
    android: &'a RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<&'a str, RoleTokens<'a>> {
    let mut roles: BTreeMap<&str, RoleTokens> = BTreeMap::new();
    for (family, is_android) in [(apple, false), (android, true)] {
        for token in &family.tokens {
            let Some(role) = token.role.as_deref() else {
                if is_android {
                    problems.push(format!("tokens.json `{}` names no role", token.name));
                }
                continue;
            };
            if is_android && (token.ios.is_some() || token.weight.is_some()) {
                problems.push(format!(
                    "tokens.json `{}` is Android's and names an ios length or a weight",
                    token.name
                ));
            }
            let tokens = roles.entry(role).or_insert(RoleTokens { apple: None, android: None });
            let (slot, platform) = if is_android {
                (&mut tokens.android, "Android")
            } else {
                (&mut tokens.apple, "Apple")
            };
            if let Some(other) = slot.replace(token) {
                problems.push(format!(
                    "tokens.json `{}` and `{}` both give {kind} role `{role}` {platform}'s length",
                    other.name, token.name
                ));
            }
        }
    }
    check_role_names(roles.keys(), problems);
    roles
}

/// A token's length, reported when it is negative.
fn length(token: &RawLengthToken, what: &str, css: &str, problems: &mut Vec<String>) -> f64 {
    let value = pixels(&token.name, what, css, problems);
    if value < 0.0 {
        problems.push(format!("tokens.json `{}`: the {what} {value} is negative", token.name));
    }
    value
}

/// An Apple token's macOS and iOS lengths.
fn apple_lengths(token: &RawLengthToken, problems: &mut Vec<String>) -> Option<(f64, f64)> {
    let Some(ios) = &token.ios else {
        problems.push(format!("tokens.json `{}` names no ios length", token.name));
        return None;
    };
    Some((
        length(token, "length", &token.value, problems),
        length(token, "ios length", ios, problems),
    ))
}

/// The icon roles: what Apple draws each at, with the weight SF Symbols draw
/// it at, and what Android draws it at.
pub(crate) fn read_icons(
    apple: &RawLengthFamily,
    android: &RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<String, IconRole> {
    read_roles("icon", apple, android, problems)
        .into_iter()
        .map(|(role, tokens)| {
            let apple = tokens.apple.and_then(|token| {
                let lengths = apple_lengths(token, problems);
                let Some(weight) = token.weight else {
                    problems.push(format!("tokens.json `{}` names no weight", token.name));
                    return None;
                };
                lengths.map(|(macos, ios)| AppleIcon { macos, ios, weight })
            });
            let android =
                tokens.android.map(|token| length(token, "length", &token.value, problems));
            (role.to_string(), IconRole { apple, android })
        })
        .collect()
}

/// The size roles: each role's lengths on every platform.
pub(crate) fn read_sizes(
    apple: &RawLengthFamily,
    android: &RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<String, PlatformLengths> {
    read_roles("size", apple, android, problems)
        .into_iter()
        .filter_map(|(role, tokens)| {
            let (Some(apple), Some(android)) = (tokens.apple, tokens.android) else {
                let platform = if tokens.apple.is_none() { "Apple" } else { "Android" };
                problems.push(format!("size role `{role}` has no {platform} token in tokens.json"));
                return None;
            };
            if apple.weight.is_some() {
                problems.push(format!(
                    "tokens.json `{}` names a weight, which only icons have",
                    apple.name
                ));
            }
            let (macos, ios) = apple_lengths(apple, problems)?;
            let android = length(android, "length", &android.value, problems);
            Some((role.to_string(), PlatformLengths { macos, ios, android }))
        })
        .collect()
}
