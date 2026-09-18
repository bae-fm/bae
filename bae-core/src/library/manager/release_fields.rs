//! Where a release's album-level values came from, and what the catalogs
//! describing it say about them.
//!
//! Three answers over the same eight fields: what the release holds now,
//! what a write leaves behind it, and what every catalog's archived document
//! states — the last of which is read back rather than stored.

use super::*;

impl LibraryManager {
    /// Carry the fields a person typed onto a projection that replaces the
    /// rest. A re-identify rewrites what a source states; what a person
    /// entered stands until they reset the release to its source.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(super) async fn carry_typed_release_fields(
        &self,
        release_id: &str,
        edit: &mut crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        let (_, release, album, _) = self.load_release_for_edit(release_id).await?;
        let stored = stored_field_values(&album, &release);
        let mut values = crate::import::FieldValues::of_edit(edit);
        for field in crate::import::CandidateEditField::ALL {
            if release.field_origins.get(field) != Some(crate::import::FieldOrigin::Typed) {
                continue;
            }
            values.set(field, stored.get(field));
            edit.origins
                .set(field, Some(crate::import::FieldOrigin::Typed));
        }
        values.apply_to(edit);
        Ok(())
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
/// What every catalog describing `records` says about the eight album-level
/// fields, read from the documents archived for each through the same mapping
/// the draft itself is read with — so a claim is exactly what resetting to
/// that catalog would put in the field.
///
/// Only the two catalogs bae asks publish documents; the rest are pages a
/// record links out to. One of those two whose document was never fetched —
/// a record another catalog's cross-link named — states nothing either.
pub(super) async fn release_field_claims(
    database: &Database,
    records: &[crate::import::ReleaseRecord],
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::FieldClaims, LibraryError> {
    let mut claimed = Vec::new();
    for record in records
        .iter()
        .filter(|record| crate::import::Catalog::LOOKUP.contains(&record.catalog))
    {
        let Some(payloads) = crate::import::payloads::load(database, &record.release_ref()).await?
        else {
            debug!(
                "no archived {} document for release {}; it states nothing about the fields",
                record.catalog.as_str(),
                record.key
            );
            continue;
        };
        claimed.push((record.catalog, payloads));
    }
    Ok(crate::import::payloads::field_claims(&claimed, clock, ids)?)
}

/// The eight album-level fields a stored release holds, as the form renders
/// them — the album's own title and year beside the release's pressing.
pub(super) fn stored_field_values(
    album: &DbAlbum,
    release: &DbRelease,
) -> crate::import::FieldValues {
    let text = |value: &Option<String>| value.clone().unwrap_or_default();
    let year = |value: Option<i32>| value.map(|year| year.to_string()).unwrap_or_default();
    crate::import::FieldValues {
        album_title: album.title.clone(),
        album_year: year(album.year),
        pressing_year: year(release.pressing.year),
        format: text(&release.pressing.format),
        label: text(&release.pressing.label),
        catalog_number: text(&release.pressing.catalog_number),
        country: text(&release.pressing.country),
        barcode: text(&release.pressing.barcode),
    }
}

/// Where each field came from once `edit` is written: what the edit itself
/// states, `Typed` for a field it changes without saying where the new value
/// came from, and the release's standing answer for a field it leaves alone. A
/// field the edit blanks came from nowhere.
///
/// The surfaces that hold a form say where each value came from; the ones that
/// build an edit field for field — MCP — say nothing, and a value they changed
/// is theirs.
pub(super) fn written_field_origins(
    edit: &crate::import::ReleaseUserEdit,
    album: &DbAlbum,
    release: &DbRelease,
) -> crate::import::FieldOrigins {
    let stored = stored_field_values(album, release);
    let written = crate::import::FieldValues::of_edit(edit);
    let mut origins = crate::import::FieldOrigins::default();
    for field in crate::import::CandidateEditField::ALL {
        let value = written.get(field).trim();
        if value.is_empty() {
            continue;
        }
        let origin = edit.origins.get(field).or_else(|| {
            if value == stored.get(field).trim() {
                release.field_origins.get(field)
            } else {
                Some(crate::import::FieldOrigin::Typed)
            }
        });
        origins.set(field, origin);
    }
    origins
}
