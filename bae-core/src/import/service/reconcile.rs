//! Reconcile a parsed release against existing library state: apply the user's
//! identity choice and edit overlay, match the release to an existing album,
//! record the import row, and remap parsed artist IDs to their real DB IDs —
//! yielding the [`super::PreparedMetadata`] the worker runs the import from.

use std::collections::HashMap;

use super::{apply_identity_choice, apply_user_edit_to_seed, ImportService, PreparedMetadata};
use crate::import::handle::remap_links;
use crate::import::ParsedWorkGraph;

impl ImportService {
    /// Reconcile the prepared release against existing library state, record the
    /// import, and remap parsed artist IDs to their real DB IDs. Pure DB work and
    /// string remapping — no network. The caller has already run the mapper its
    /// identity choice calls for, so the input is a mapped `ParsedAlbum` plus its
    /// raw metadata pairs (empty for Unknown).
    ///
    /// The `identity_choice` post-process runs on top of the mapper's output:
    ///
    /// - **Exact** — passes through. Identity rows keep their
    ///   `source_release_id`, and pressing-level metadata seeds from the picked
    ///   release.
    /// - **Approximate** — identity rows get `source_release_id = None`, and
    ///   pressing-level metadata is cleared, so the release row reflects "the
    ///   user didn't claim a specific pressing". Album-group-stable fields
    ///   (title, artist, tracks) still come from the picked release.
    /// - **Unknown** — passes through, empty identity vec and all. The album
    ///   lookup is skipped (no identities to match), so the release lands on a
    ///   fresh album.
    ///
    /// For Exact / Approximate, `metadata_source` and
    /// `metadata_source_release_id` keep pointing at the picked release — the
    /// release records which source release seeded it regardless of the claim.
    /// Unknown arrives with those columns already set by its mapper.
    ///
    /// The confirmation-page `user_edit` overlay applies last, so the user's
    /// edits win over every seeded value.
    pub(super) async fn reconcile_prepared_release(
        &self,
        parsed: crate::import::ParsedAlbum,
        identity_choice: &crate::import::IdentityChoice,
        user_edit: Option<crate::import::ReleaseUserEdit>,
        replacement_release_ids: &[String],
    ) -> Result<PreparedMetadata, crate::import::ImportError> {
        let library_manager = &self.library_manager;

        let crate::import::ParsedAlbum {
            album: mut db_album,
            release: mut db_release,
            tracks: mut db_tracks,
            mut artists,
            mut album_artists,
            mut track_artists,
            work_graph,
            release_artist_roles,
            track_artist_roles,
            identities: parsed_identities,
        } = parsed;

        let identities = apply_identity_choice(&parsed_identities, identity_choice);
        if matches!(
            identity_choice,
            crate::import::IdentityChoice::Approximate { .. }
        ) {
            // `disc_id` isn't part of the pressing cluster — it's a signal, which
            // the pipeline supplies separately from the user's own LOG/CUE
            // artifacts. Wiping the source's value here keeps the seeded record's
            // signals reflecting only the user's physical media.
            db_release.pressing = crate::db::Pressing::blank();
            db_release.disc_id = None;
        }

        // The overlay applies after the choice, so a field Approximate just
        // cleared can still be filled in by the user before commit.
        if let Some(edit) = user_edit {
            apply_user_edit_to_seed(
                &edit,
                &mut db_album,
                &mut db_release,
                &mut db_tracks,
                &mut artists,
                &mut album_artists,
                &mut track_artists,
                self.clock.as_ref(),
                self.ids.as_ref(),
            )?;
        }

        let album_title = db_album.title.clone();
        let artist_name = artists
            .iter()
            .find(|a| a.id == db_album.artist_id)
            .expect("primary artist must be in artists vec")
            .name
            .clone();

        let existing_album_id = library_manager
            .find_existing_album_for_import_excluding(&identities, replacement_release_ids)
            .await?;
        if let Some(album_id) = &existing_album_id {
            db_release.album_id = album_id.clone();
        }

        let resolved = library_manager.resolve_artists_for_import(&artists).await?;

        let artist_id_map: HashMap<String, String> = artists
            .iter()
            .zip(resolved.ids.iter())
            .map(|(a, id)| (a.id.clone(), id.clone()))
            .collect();

        let remapped_primary_artist_id = artist_id_map
            .get(&db_album.artist_id)
            .ok_or_else(|| crate::import::ImportError::Internal {
                detail: format!(
                    "Primary artist ID {} not found in artist map",
                    db_album.artist_id
                ),
            })?
            .clone();
        db_album.artist_id = remapped_primary_artist_id;

        let remapped_track_artists = remap_links(
            &track_artists,
            &artist_id_map,
            "track artist",
            |ta| &ta.artist_id,
            |ta, artist_id| ta.artist_id = artist_id,
        )?;
        let remapped_album_artists = if existing_album_id.is_none() {
            remap_links(
                &album_artists,
                &artist_id_map,
                "album artist",
                |aa| &aa.artist_id,
                |aa, artist_id| aa.artist_id = artist_id,
            )?
        } else {
            vec![]
        };
        let remapped_release_artist_roles = remap_links(
            &release_artist_roles,
            &artist_id_map,
            "release artist role",
            |role| &role.artist_id,
            |role, artist_id| role.artist_id = artist_id,
        )?;
        let remapped_track_artist_roles = remap_links(
            &track_artist_roles,
            &artist_id_map,
            "track artist role",
            |role| &role.artist_id,
            |role, artist_id| role.artist_id = artist_id,
        )?;

        // A work performed by an already-imported release keeps that release's
        // `works` row, so every link this import writes points at the resolved id
        // and only the works new to the library are inserted.
        let resolved_works = library_manager
            .resolve_works_for_import(&work_graph.works)
            .await?;
        let work_id_map: HashMap<String, String> = work_graph
            .works
            .iter()
            .zip(resolved_works.ids.iter())
            .map(|(work, id)| (work.id.clone(), id.clone()))
            .collect();

        // A work_artists row points at both an artist and a work, and a
        // work_parts row at two works, so each is remapped once per endpoint.
        let work_artists_by_artist = remap_links(
            &work_graph.work_artists,
            &artist_id_map,
            "work artist",
            |link| &link.artist_id,
            |link, artist_id| link.artist_id = artist_id,
        )?;
        let remapped_work_artists = remap_links(
            &work_artists_by_artist,
            &work_id_map,
            "work artist work",
            |link| &link.work_id,
            |link, work_id| link.work_id = work_id,
        )?;
        let work_parts_by_parent = remap_links(
            &work_graph.work_parts,
            &work_id_map,
            "work part parent",
            |part| &part.parent_work_id,
            |part, work_id| part.parent_work_id = work_id,
        )?;
        let remapped_work_parts = remap_links(
            &work_parts_by_parent,
            &work_id_map,
            "work part child",
            |part| &part.child_work_id,
            |part, work_id| part.child_work_id = work_id,
        )?;
        let remapped_track_works = remap_links(
            &work_graph.track_works,
            &work_id_map,
            "track work",
            |link| &link.work_id,
            |link, work_id| link.work_id = work_id,
        )?;

        let artist_images = library_manager
            .fetch_discogs_artist_images(&artists, &artist_id_map)
            .await;

        let artist_inserts = resolved.inserts;
        let artist_external_id_updates = resolved.external_id_updates;

        Ok(PreparedMetadata {
            db_album,
            db_release,
            db_tracks,
            remote_cover_image: None,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            work_graph: ParsedWorkGraph {
                works: resolved_works.inserts,
                work_artists: remapped_work_artists,
                work_parts: remapped_work_parts,
                track_works: remapped_track_works,
            },
            remapped_release_artist_roles,
            remapped_track_artist_roles,
            artists: artist_inserts,
            artist_external_id_updates,
            artist_images,
            identities,
            album_title,
            artist_name,
        })
    }
}
