//! What a release editor's save and a metadata edit read, and what their
//! writes refuse.

use super::tests::{insert_editable_tracks, titled};
use crate::manager::tests::*;

/// A save reads everything it starts from in one statement, and a metadata
/// edit the rows it writes over in another one. Each edit's write runs its
/// statement again: once the release changed, it refuses and writes nothing.
#[tokio::test]
async fn an_edit_reads_its_inputs_once_and_its_write_refuses_a_changed_release() {
    let (test, _temp_dir) = setup_test_manager().await;
    let manager = test.library.manager();
    let album = create_test_album();
    let release = create_test_release(&album.id);
    manager.records.insert_album(&album).await.unwrap();
    insert_release(manager, &release).await;
    insert_editable_tracks(manager, &release.id, 2).await;
    let form = manager.open_release_edit(&release.id).await.unwrap();

    let counter = manager.store.track_work_for_test().await.unwrap();
    let inputs =
        manager.import_tables.read_release_save_inputs(&form).await.unwrap().expect("inputs");
    assert_eq!(counter.take().app_read_statements, 1, "a save's inputs");
    let read =
        manager.records.find_release_to_edit(&release.id).await.unwrap().expect("the release");
    assert_eq!(counter.take().app_read_statements, 1, "a metadata edit's inputs");
    drop(counter);
    assert_eq!(inputs.seed, form.opened);
    assert_eq!(inputs.release, read);
    assert_eq!(inputs.facts.files.len(), 2);

    let metadata = manager
        .release_metadata_rows(
            read.album.clone(),
            read.release.clone(),
            &read.tracks,
            &bae_import::ReleaseUserEdit {
                metadata: titled(form.clone(), "Saved Over It").edit.shape().unwrap().metadata,
                facts: read.release.pressing.facts.clone(),
            },
        )
        .unwrap();
    let unchanged = |inputs: bae_import::tables::ReleaseSaveInputs| {
        let form = form.clone();
        async move {
            manager
                .store
                .write(move |sql| {
                    bae_import::tables::release_save_inputs_unchanged_on(&sql, &form, &inputs)
                })
                .await
                .unwrap()
        }
    };
    assert!(unchanged(inputs).await.is_ok(), "nothing changed since the read");
    let inputs =
        manager.import_tables.read_release_save_inputs(&form).await.unwrap().expect("inputs");
    manager.records.rename_track_for_test(&read.tracks[0].id, "Renamed Elsewhere").await.unwrap();

    let stale = unchanged(inputs).await;
    assert!(matches!(stale, Err(bae_records::ReleaseEditRefusal::Stale)), "{stale:?}");
    let refused = manager.records.update_release_metadata_user_edit(metadata, read).await;
    assert!(
        matches!(
            refused,
            Err(bae_store::WriteError::Refused(bae_records::ReleaseEditRefusal::Stale))
        ),
        "{refused:?}"
    );
    let held = manager.open_release_edit(&release.id).await.unwrap();
    assert_ne!(held.opened.edit.album_title, "Saved Over It");
}
