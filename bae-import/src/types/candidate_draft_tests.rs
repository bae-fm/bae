//! A candidate's draft values as they are stored.

use super::*;
use bae_catalogs::catalog::LookupCatalog;

#[test]
fn a_metadata_provenance_round_trips_without_an_identity_proxy() {
    let provenance = MetadataProvenance::ExternalRelease {
        record: bae_catalogs::records::LookupRef::new(
            LookupCatalog::MusicBrainz,
            "release-a".to_string(),
        ),
    };
    let stored = serde_json::to_string(&provenance).expect("metadata provenance encodes");
    let read_back: MetadataProvenance =
        serde_json::from_str(&stored).expect("stored metadata provenance decodes");
    assert_eq!(read_back, provenance);
}
