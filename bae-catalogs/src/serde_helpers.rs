use serde::{Deserialize, Deserializer};

/// Map an absent or blank string field to `None`.
pub(crate) fn empty_string_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?
        .and_then(|value| (!value.trim().is_empty()).then_some(value)))
}
