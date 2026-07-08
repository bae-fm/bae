//! Reconcile a parsed release against existing library state.
//!
//! Applies the user's identity choice and edit overlay, matches the release
//! to an existing album, records the import row, and remaps parsed artist IDs
//! to their real DB IDs, yielding the [`super::PreparedMetadata`] the worker
//! hands to the storage pass.

use std::collections::HashMap;

use super::{apply_identity_choice, apply_user_edit_to_seed, ImportService, PreparedMetadata};
use crate::db::DbImport;
use crate::import::handle::{
    fetch_artist_images, remap_album_artists, remap_artist_links, remap_track_artists,
};
use crate::import::ParsedWorkGraph;

impl ImportService {
    /// Reconcile the prepared release against existing library state,
    /// record the import, and remap parsed artist IDs to their actual
    /// DB IDs.
    ///
    /// Input is the already-mapped `ParsedAlbum` plus the raw metadata
    /// pairs (empty for Unknown). The caller chooses the mapper based
    /// on identity choice — `prepare_release` for Exact / Approximate,
    /// `map_unknown_candidate_to_db` for Unknown. This function does pure DB
    /// work and string remapping, no network.
    ///
    /// Applies the user's `identity_choice` post-process on top of the
    /// mapper output:
    ///
    /// - **Exact** — mapper output as-is. Identity rows keep their
    ///   `source_release_id`; pressing-level metadata (year, format,
    ///   label, catalog_number, country) seeds from the picked
    ///   release.
    /// - **Approximate** — identity rows get `source_release_id = None`
    ///   (cross-source rows from url-rels mirror the user's choice and
    ///   also become group-only); pressing-level metadata is cleared
    ///   so the release row reflects "user didn't claim a specific
    ///   pressing." Album-group-stable fields (title, artist, tracks)
    ///   stay populated from the picked release.
    /// - **Unknown** — mapper output (empty identity vec, file-tag
    ///   release fields) passes through. `metadata_source` is
    ///   `'file_tags'`; `metadata_source_release_id` stays NULL.
    ///   The album lookup is skipped (empty identities), so the
    ///   release lands on a fresh album.
    ///
    /// For Exact / Approximate, `metadata_source` and
    /// `metadata_source_release_id` are kept pointing at the picked
    /// release — the release records which source release seeded it
    /// regardless of identity claim. For Unknown those columns
    /// already arrive set by the Unknown mapper.
    ///
    /// `user_edit` is an optional overlay from the confirmation-page
    /// editor. Applied after the choice transformation so the user's
    /// edits win over the seeded values.
    ///
    /// Shared between folder and CD imports; callers handle the parts
    /// that differ (cover art, file discovery, ripping).
    pub(super) async fn reconcile_prepared_release(
        &self,
        parsed: crate::import::ParsedAlbum,
        resolved_metadata: Vec<(String, String)>,
        import_id: &str,
        source_path: &str,
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

        // Apply the identity choice on top of the mapper's output.
        // Exact / Unknown preserve the mapper's per-source release IDs
        // (Unknown's vec is empty anyway); Approximate NULLs them and
        // clears the pressing-level cluster so the seeded record
        // reflects "user claimed an album, not a pressing."
        let identities = apply_identity_choice(&parsed_identities, identity_choice);
        if matches!(
            identity_choice,
            crate::import::IdentityChoice::Approximate { .. }
        ) {
            // `disc_id` is a signal-domain field, not part of the
            // pressing cluster — the import pipeline supplies it from
            // the user's actual LOG/CUE artifacts separately, so wiping
            // the source's value keeps the seeded record's signals
            // reflecting only the user's physical media.
            db_release.pressing = crate::db::Pressing::blank();
            db_release.disc_id = None;
        }

        // User-edit overlay: apply the user's edits on top of the seed.
        // Done here so reseeded fields (Approximate cleared) can still
        // be user-overridden via the editor before commit. Returns
        // updated db_album / db_release / db_tracks / album_artists /
        // track_artists / artists with new artist rows merged in.
        if let Some(edit) = user_edit {
            apply_user_edit_to_seed(
                &edit,
                &mut db_album,
                &mut db_release,
                &mut db_tracks,
                &mut artists,
                &mut album_artists,
                &mut track_artists,
                library_manager.clock().as_ref(),
                library_manager.ids().as_ref(),
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

        let db_import = DbImport::new(
            import_id,
            &album_title,
            &artist_name,
            source_path,
            library_manager.clock().now(),
        );
        library_manager.insert_import(&db_import).await?;

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

        let remapped_track_artists = remap_track_artists(&track_artists, &artist_id_map)?;
        let remapped_album_artists = if existing_album_id.is_none() {
            remap_album_artists(&album_artists, &artist_id_map)?
        } else {
            vec![]
        };
        let remapped_work_artists = remap_artist_links(
            &work_graph.work_artists,
            &artist_id_map,
            "work artist",
            |link| &link.artist_id,
            |link, artist_id| link.artist_id = artist_id,
        )?;
        let remapped_release_artist_roles = remap_artist_links(
            &release_artist_roles,
            &artist_id_map,
            "release artist role",
            |role| &role.artist_id,
            |role, artist_id| role.artist_id = artist_id,
        )?;
        let remapped_track_artist_roles = remap_artist_links(
            &track_artist_roles,
            &artist_id_map,
            "track artist role",
            |role| &role.artist_id,
            |role, artist_id| role.artist_id = artist_id,
        )?;

        let discogs_client = library_manager.discogs_client()?;
        let artist_images = if let Some(ref discogs_client) = discogs_client {
            fetch_artist_images(library_manager, discogs_client, &artists, &artist_id_map).await
        } else {
            Vec::new()
        };

        let artist_inserts = resolved.inserts;
        let artist_external_id_updates = resolved.external_id_updates;

        Ok(PreparedMetadata {
            db_album,
            db_release,
            db_tracks,
            remote_cover_image: None,
            resolved_metadata,
            existing_album_id,
            remapped_track_artists,
            remapped_album_artists,
            work_graph: ParsedWorkGraph {
                works: work_graph.works,
                work_artists: remapped_work_artists,
                work_parts: work_graph.work_parts,
                track_works: work_graph.track_works,
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
