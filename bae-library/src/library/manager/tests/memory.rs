use super::*;

/// Releasing the store's cached pages leaves the library readable and
/// writable: reads after it load what they need again.
#[tokio::test]
async fn release_memory_keeps_the_library_readable_and_writable() {
    let (manager, _temp_dir, album, release) = manager_with_release().await;
    assert!(find_release(&manager, &release.id).await.is_some());

    manager.release_memory().await.unwrap();

    assert_eq!(find_album(&manager, &album.id).await.unwrap().id, album.id);
    let second = create_test_release(&album.id);
    insert_release(&manager, &second).await;
    assert_eq!(album_releases(&manager, &album.id).await.len(), 2);
}
