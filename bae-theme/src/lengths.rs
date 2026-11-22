//! The opacity, radius, spacing, icon and size roles of
//! design/system/tokens.json. An opacity is a fraction every platform shares,
//! a token in the `opacity` family naming its role in `role`. The
//! radii and the spacing are one length every platform shares, a token in the
//! `radius` or `spacing` family naming its role in `role`; one with no `role`,
//! such as Android's Material shapes, is a measure only the design system
//! draws. An icon or size role is a token in
//! an Apple family (`icon`, `size`), whose value is the macOS length and whose
//! `ios` is iOS's, and a token in its Android family (`icon-android`,
//! `size-android`), whose value is Android's; each names the role in `role`.
//! An icon role may be one platform's alone, as a text role may; a size role
//! has both. A token with no `role` is a design-system measure no app draws as
//! a role, such as a macOS control's height.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{
    pair_roles, pixels, AppleIcon, IconRole, PlatformLengths, RoleEntry, RolePair, Weight,
};

/// A family of length tokens.
#[derive(Deserialize)]
pub(crate) struct RawLengthFamily {
    tokens: Vec<RawLengthToken>,
}

#[derive(Deserialize)]
pub(crate) struct RawLengthToken {
    name: String,
    role: Option<String>,
    value: String,
    ios: Option<String>,
    /// On Apple, the weight SF Symbols draw an icon at.
    weight: Option<Weight>,
}

/// The tokens of an Apple and an Android family of <kind> tokens, by role.
fn read_roles<'a>(
    kind: &str,
    apple: &'a RawLengthFamily,
    android: &'a RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<&'a str, RolePair<&'a RawLengthToken>> {
    let mut entries = Vec::new();
    for (family, is_android) in [(apple, false), (android, true)] {
        for token in &family.tokens {
            // A token with no role is a measure only the design system draws.
            let Some(role) = token.role.as_deref() else {
                continue;
            };
            if is_android && (token.ios.is_some() || token.weight.is_some()) {
                problems.push(format!(
                    "tokens.json `{}` is Android's and names an ios length or a weight",
                    token.name
                ));
            }
            entries.push(RoleEntry { role, name: &token.name, android: is_android, value: token });
        }
    }
    pair_roles(kind, entries, problems)
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

/// The roles of a family of <kind> tokens every platform shares, such as the
/// radii or the opacities: each role's value, which <value> reads from its
/// token.
pub(crate) fn read_shared(
    kind: &str,
    family: &RawLengthFamily,
    value: fn(&RawLengthToken, &mut Vec<String>) -> f64,
    problems: &mut Vec<String>,
) -> BTreeMap<String, f64> {
    let mut entries = Vec::new();
    for token in &family.tokens {
        // A token with no role is a measure only the design system draws.
        let Some(role) = token.role.as_deref() else {
            continue;
        };
        if token.ios.is_some() || token.weight.is_some() {
            problems.push(format!(
                "tokens.json `{}` is every platform's and names an ios length or a weight",
                token.name
            ));
        }
        entries.push(RoleEntry { role, name: &token.name, android: false, value: token });
    }
    pair_roles(kind, entries, problems)
        .into_iter()
        .map(|(role, pair)| {
            let token = pair.apple.unwrap_or_else(|| {
                panic!("{kind} role `{role}`: read_shared pairs only Apple entries, so each role has one")
            });
            (role.to_string(), value(token, problems))
        })
        .collect()
}

/// A shared token's length, as a radius or a spacing gives it.
pub(crate) fn shared_length(token: &RawLengthToken, problems: &mut Vec<String>) -> f64 {
    length(token, "length", &token.value, problems)
}

/// A shared token's opacity, a fraction between 0 and 1.
pub(crate) fn opacity(token: &RawLengthToken, problems: &mut Vec<String>) -> f64 {
    match token.value.parse::<f64>() {
        Ok(value) if (0.0..=1.0).contains(&value) => value,
        _ => {
            problems.push(format!(
                "tokens.json `{}`: `{}` is not an opacity between 0 and 1",
                token.name, token.value
            ));
            0.0
        }
    }
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
