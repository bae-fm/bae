//! The icon roles of design/system/tokens.json. Each role is a token in the
//! Apple family `icon`, whose value is the macOS length and whose `ios` is
//! iOS's, and a token in the Android family `icon-android`, whose value is
//! Android's; each names the role in `role`. An Apple token with no `role` is
//! a design-system measure no app draws as a role.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{check_role_names, pixels, IconRole, PlatformLengths, Weight};

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

/// A role's lengths, and the weight its Apple token names.
struct RoleLengths {
    lengths: PlatformLengths,
    weight: Option<Weight>,
}

/// The roles of an Apple and an Android family of <kind> tokens.
fn read_roles(
    kind: &str,
    apple: &RawLengthFamily,
    android: &RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<String, RoleLengths> {
    let mut apple_roles: BTreeMap<&str, &RawLengthToken> = BTreeMap::new();
    let mut android_roles: BTreeMap<&str, &RawLengthToken> = BTreeMap::new();
    for (tokens, roles, platform) in
        [(apple, &mut apple_roles, "Apple"), (android, &mut android_roles, "Android")]
    {
        for token in &tokens.tokens {
            let Some(role) = token.role.as_deref() else {
                if platform == "Android" {
                    problems.push(format!("tokens.json `{}` names no role", token.name));
                }
                continue;
            };
            if let Some(other) = roles.insert(role, token) {
                problems.push(format!(
                    "tokens.json `{}` and `{}` both give {kind} role `{role}` {platform}'s length",
                    other.name, token.name
                ));
            }
        }
    }
    check_role_names(apple_roles.keys().chain(android_roles.keys()), problems);
    for (role, token) in &android_roles {
        if token.ios.is_some() || token.weight.is_some() {
            problems.push(format!(
                "tokens.json `{}` is Android's and names an ios length or a weight",
                token.name
            ));
        }
        if !apple_roles.contains_key(role) {
            problems.push(format!("{kind} role `{role}` has no Apple token in tokens.json"));
        }
    }
    apple_roles
        .into_iter()
        .filter_map(|(role, token)| {
            let Some(android) = android_roles.get(role) else {
                problems.push(format!("{kind} role `{role}` has no Android token in tokens.json"));
                return None;
            };
            let Some(ios) = &token.ios else {
                problems.push(format!("tokens.json `{}` names no ios length", token.name));
                return None;
            };
            let lengths = PlatformLengths {
                macos: pixels(&token.name, "length", &token.value, problems),
                ios: pixels(&token.name, "ios length", ios, problems),
                android: pixels(&android.name, "length", &android.value, problems),
            };
            for (platform, length) in
                [("macOS", lengths.macos), ("iOS", lengths.ios), ("Android", lengths.android)]
            {
                if length < 0.0 {
                    problems.push(format!("{kind} role `{role}`: {platform}'s length is negative"));
                }
            }
            Some((role.to_string(), RoleLengths { lengths, weight: token.weight }))
        })
        .collect()
}

/// The icon roles: each role's lengths, and the weight SF Symbols draw it at.
pub(crate) fn read_icons(
    apple: &RawLengthFamily,
    android: &RawLengthFamily,
    problems: &mut Vec<String>,
) -> BTreeMap<String, IconRole> {
    read_roles("icon", apple, android, problems)
        .into_iter()
        .filter_map(|(role, read)| match read.weight {
            Some(weight) => Some((role, IconRole { weight, sizes: read.lengths })),
            None => {
                problems.push(format!("icon role `{role}` names no weight in tokens.json"));
                None
            }
        })
        .collect()
}
