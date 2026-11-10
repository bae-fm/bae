use super::*;

/// A PNG of `width` by `height` pixels in one color.
fn solid_png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    png(image::RgbImage::from_pixel(width, height, image::Rgb(color)))
}

fn png(image: image::RgbImage) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[tokio::test]
async fn a_profile_names_each_new_avatar_at_once() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    let people = library.people().clone();
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Rowan").await;
    assert_eq!(library.people().avatar(profile.id).await.unwrap(), None);
    let first_source = solid_png(16, 16, [120, 40, 200]);
    let second_source = solid_png(16, 16, [20, 160, 90]);
    let first_jpeg = bae_util::stored_image::stored_jpeg(&first_source).unwrap();
    let second_jpeg = bae_util::stored_image::stored_jpeg(&second_source).unwrap();
    // Coven sends a file a row references with the write that references it,
    // paused uploads or not, so the files stay on this device only for as
    // long as the test holds the provider's creates.
    let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
    storage.gate_creations(gate.clone()).await;
    library.manager().set_avatar(Some(first_source)).await.unwrap();
    let avatar = async || people.profile(profile.id).await.unwrap().unwrap().avatar;
    let first = avatar().await.expect("the first avatar");
    assert_eq!(library.people().avatar(profile.id).await.unwrap().unwrap(), first_jpeg);
    library.manager().set_avatar(Some(second_source)).await.unwrap();
    let second = avatar().await.expect("the second avatar");
    // The profile row references each new avatar's coven file from the write
    // that sets it; a new avatar is a new file.
    assert_ne!(first.file, second.file);
    // Its version is the file's, naming this device too while the file is on
    // it only.
    assert!(second.version.starts_with(&format!("{}@", second.file.0)), "{}", second.version);
    assert_eq!(library.people().avatar(profile.id).await.unwrap().unwrap(), second_jpeg);
    gate.add_permits(tokio::sync::Semaphore::MAX_PERMITS);
    let mut profiles = library.manager().subscribe_profiles();
    let stored = bae_library::followed::value_matching(&mut profiles, |profiles| {
        profiles.iter().find(|candidate| candidate.id == profile.id).is_some_and(|candidate| {
            candidate
                .avatar
                .as_ref()
                .is_some_and(|avatar| avatar.version == second.file.0.to_string())
        })
    })
    .await
    .into_iter()
    .find(|candidate| candidate.id == profile.id)
    .and_then(|candidate| candidate.avatar)
    .expect("the stored avatar");
    assert_eq!(stored.file, second.file, "the upload changes no row");
    assert_eq!(
        stored.version,
        second.file.0.to_string(),
        "the version moves to the file's alone as the file is stored"
    );
    assert_eq!(library.people().avatar(profile.id).await.unwrap().unwrap(), second_jpeg);
    library.manager().set_avatar(None).await.unwrap();
    assert_eq!(library.people().avatar(profile.id).await.unwrap(), None);
    assert_eq!(avatar().await, None);
    library.manager().close().await.unwrap();
}

/// A camera-sized photo is stored as a cover is: a JPEG downscaled to fit the
/// stored edge, its aspect kept. Its pixels vary everywhere, so neither the
/// PNG source nor the JPEG compresses it away.
#[tokio::test]
async fn an_oversized_avatar_is_stored_as_a_downscaled_jpeg() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Rowan").await;
    let edge = bae_util::stored_image::STORED_MAX_EDGE;
    let photo = png(image::RgbImage::from_fn(edge * 5, edge * 10 / 3, |x, y| {
        let mixed = x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(2_246_822_519);
        let [r, g, b, _] = mixed.to_le_bytes();
        image::Rgb([r, g, b])
    }));

    library.manager().set_avatar(Some(photo)).await.unwrap();

    let stored = library.people().avatar(profile.id).await.unwrap().unwrap();
    assert_eq!(image::guess_format(&stored).unwrap(), image::ImageFormat::Jpeg);
    let decoded = image::load_from_memory(&stored).unwrap();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (edge, edge * 2 / 3),
        "downscaled to fit the stored edge, aspect kept"
    );
    library.manager().close().await.unwrap();
}

/// A picked PNG is stored as a cover or an artist picture would be: the
/// stored bytes are the JPEG bae makes of any image it stores.
#[tokio::test]
async fn a_png_avatar_is_stored_as_covers_are() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Rowan").await;
    let picked = solid_png(16, 16, [120, 40, 200]);

    library.manager().set_avatar(Some(picked.clone())).await.unwrap();

    let stored = library.people().avatar(profile.id).await.unwrap().unwrap();
    assert_eq!(stored, bae_util::stored_image::stored_jpeg(&picked).unwrap());
    assert_eq!(image::guess_format(&stored).unwrap(), image::ImageFormat::Jpeg);
    library.manager().close().await.unwrap();
}

/// Bytes that are no image make no avatar: the setting is refused and the
/// profile keeps none.
#[tokio::test]
async fn an_undecodable_avatar_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let (library, storage) = storage_library(
        directory.path(),
        bae_catalogs::fake_catalogs::FakeCatalogs::offline().providers(),
        bae_util::http::Http::for_test(),
    )
    .await;
    connect_storage(library.manager(), library.store(), &storage).await;
    let profile = made_profile(library.manager(), library.people(), "Rowan").await;

    let error = library
        .manager()
        .set_avatar(Some(b"not an image".to_vec()))
        .await
        .expect_err("bytes that are no image make no avatar");

    assert!(matches!(error, bae_library::people::AvatarError::Image { .. }), "{error}");
    assert_eq!(library.people().avatar(profile.id).await.unwrap(), None);
    library.manager().close().await.unwrap();
}
