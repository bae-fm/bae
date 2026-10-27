/// What the context lane plays from: a single release, several releases one
/// after another in the order they were chosen, or the whole library. The
/// queue screens label the lane by it; the lane's tracks are in the queue
/// tables, filled once when it starts playing.
///
/// `Releases` always holds at least two ids: a single-release play collapses to
/// `Release` (see [`ContextSource::releases`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextSource {
    Release(String),
    Releases(Vec<String>),
    Library,
}

/// The `playback_queue.source` value standing for the library. A release id is
/// a UUID, which can never equal this literal, so the two are unambiguous in
/// the column.
const LIBRARY_SOURCE: &str = "library";

impl ContextSource {
    /// Build a source from an ordered, non-empty set of release ids, collapsing a
    /// single id to [`ContextSource::Release`] so a one-album play behaves exactly
    /// like playing that release. The caller guarantees a non-empty list.
    pub fn releases(mut ids: Vec<String>) -> ContextSource {
        debug_assert!(!ids.is_empty(), "a release source needs at least one id");
        if ids.len() == 1 {
            ContextSource::Release(ids.remove(0))
        } else {
            ContextSource::Releases(ids)
        }
    }

    /// Encode the source for the `playback_queue.source` column: a release
    /// stores its id, several releases a JSON array of their ids, the library
    /// the `LIBRARY_SOURCE` word. The three are mutually unambiguous — a
    /// release id is a UUID (never `[…]`, never the word), and a JSON array
    /// always starts with `[`.
    pub(crate) fn to_column(&self) -> String {
        match self {
            ContextSource::Release(id) => id.clone(),
            ContextSource::Releases(ids) => {
                serde_json::to_string(ids).expect("release ids serialize to a JSON array")
            }
            ContextSource::Library => LIBRARY_SOURCE.to_string(),
        }
    }

    /// Decode the `playback_queue.source` column. A value that opens like a
    /// JSON array but isn't one, or is an empty one, is not something
    /// [`Self::to_column`] writes.
    pub(crate) fn from_column(value: &str) -> Result<ContextSource, String> {
        if value == LIBRARY_SOURCE {
            return Ok(ContextSource::Library);
        }
        if value.starts_with('[') {
            let ids: Vec<String> = serde_json::from_str(value).map_err(|e| {
                format!("the queue's source {value:?} is not a JSON array of release ids: {e}")
            })?;
            if ids.is_empty() {
                return Err("the queue's source is an empty array of release ids".to_string());
            }
            return Ok(ContextSource::releases(ids));
        }
        Ok(ContextSource::Release(value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_of_one_collapses_to_release() {
        assert_eq!(
            ContextSource::releases(vec!["only".into()]),
            ContextSource::Release("only".into())
        );
    }

    #[test]
    fn releases_of_many_keeps_input_order() {
        assert_eq!(
            ContextSource::releases(vec!["a".into(), "b".into(), "c".into()]),
            ContextSource::Releases(vec!["a".into(), "b".into(), "c".into()])
        );
    }

    /// Every source kind survives the column's encode and decode, and none is
    /// mistaken for another.
    #[test]
    fn source_column_round_trips() {
        for source in [
            ContextSource::Release("550e8400-e29b-41d4-a716-446655440000".into()),
            ContextSource::Releases(vec![
                "550e8400-e29b-41d4-a716-446655440000".into(),
                "6ba7b810-9dad-11d1-80b4-00c04fd430c8".into(),
            ]),
            ContextSource::Library,
        ] {
            assert_eq!(ContextSource::from_column(&source.to_column()), Ok(source));
        }
    }

    /// A one-element array decodes as the release, as `releases` builds it.
    #[test]
    fn single_element_release_array_decodes_as_release() {
        let encoded = serde_json::to_string(&["only-one"]).unwrap();
        assert_eq!(
            ContextSource::from_column(&encoded),
            Ok(ContextSource::Release("only-one".into()))
        );
    }

    #[test]
    fn malformed_or_empty_source_array_is_refused() {
        assert!(ContextSource::from_column("[not valid json").is_err());
        assert!(ContextSource::from_column("[]").is_err());
    }
}
