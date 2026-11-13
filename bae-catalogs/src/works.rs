//! The works a release's tracks perform, as MusicBrainz relates them.

use crate::credits::ArtistCredit;
use bae_types::credit_role::CreditedAs;

/// The works a release's tracks perform, and how they nest. Works come from
/// MusicBrainz, which names them; each is held once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkGraph {
    pub works: Vec<GraphWork>,
    /// Which work is part of which, in the order the source relates them.
    pub parts: Vec<WorkPart>,
}

/// One MusicBrainz work as its source states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphWork {
    pub musicbrainz_work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    /// Its credits, in the work's order.
    pub credits: Vec<WorkCredit>,
}

/// One artist credited on a work, and what the artist did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkCredit {
    pub artist: ArtistCredit,
    pub role: CreditedAs,
}

/// One work that is part of another, both named by their MusicBrainz ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkPart {
    pub parent: String,
    pub child: String,
}

impl WorkGraph {
    /// The graph holding only the works `performed` names and the works they
    /// nest among, transitively: what a set of tracks performs once the
    /// tracks that performed the rest are gone.
    pub fn keeping_performed<'a>(&self, performed: impl IntoIterator<Item = &'a str>) -> Self {
        let mut kept: std::collections::HashSet<&str> = performed.into_iter().collect();
        loop {
            let before = kept.len();
            for part in &self.parts {
                if kept.contains(part.parent.as_str()) || kept.contains(part.child.as_str()) {
                    kept.insert(&part.parent);
                    kept.insert(&part.child);
                }
            }
            if kept.len() == before {
                break;
            }
        }
        Self {
            works: self
                .works
                .iter()
                .filter(|work| kept.contains(work.musicbrainz_work_id.as_str()))
                .cloned()
                .collect(),
            parts: self
                .parts
                .iter()
                .filter(|part| kept.contains(part.parent.as_str()))
                .cloned()
                .collect(),
        }
    }
}
