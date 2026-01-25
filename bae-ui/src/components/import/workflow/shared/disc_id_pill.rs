//! Disc ID pill component

use crate::components::{Pill, PillVariant};
use dioxus::prelude::*;

/// A clickable pill that displays a MusicBrainz disc ID and links to its page
#[component]
pub fn DiscIdPill(disc_id: String) -> Element {
    rsx! {
        Pill {
            variant: PillVariant::Link,
            href: "https://musicbrainz.org/cdtoc/{disc_id}",
            monospace: true,
            "{disc_id}"
        }
    }
}
