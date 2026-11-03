// ── The album picker: a picked library album over the one the records name ──

/// A candidate with no records files under a new album until the person
/// picks one; the pick shows on the pane, and clearing it leaves the
/// candidate to its records again.
#[tokio::test(flavor = "multi_thread")]
async fn a_picked_album_shows_over_what_the_records_name() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    assert_eq!(pane(&handle, &key).await.draft().unwrap().album, crate::DraftAlbum::New);

    let artist = make_artist("Artist Name", None, None);
    handle.database.insert_artist(&artist).await.unwrap();
    let (album, _release) =
        insert_library_release(&handle.database, &artist.id, "Library Album").await;

    handle.set_candidate_album(&key, Some(album.id.clone())).await.unwrap();
    assert_eq!(
        pane(&handle, &key).await.draft().unwrap().album,
        crate::DraftAlbum::Picked {
            album_id: album.id.clone(),
            title: "Library Album".to_string(),
        }
    );

    handle.set_candidate_album(&key, None).await.unwrap();
    let after = pane(&handle, &key).await.draft().unwrap().album.clone();
    shut_down(handle).await;
    assert_eq!(after, crate::DraftAlbum::New);
}

/// An album no longer in the library cannot be picked.
#[tokio::test(flavor = "multi_thread")]
async fn an_album_not_in_the_library_cannot_be_picked() {
    let (handle, _tmp, key, _hash) = pane_fixture().await;
    let picked = handle
        .set_candidate_album(&key, Some("0b7e1f1e-0000-4000-8000-000000000000".to_string()))
        .await;
    let after = pane(&handle, &key).await.draft().unwrap().album.clone();
    shut_down(handle).await;
    assert!(picked.is_err());
    assert_eq!(after, crate::DraftAlbum::New);
}
