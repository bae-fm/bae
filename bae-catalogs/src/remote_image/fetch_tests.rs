use super::*;
use crate::remote_image::tests::*;
use crate::remote_image::RemoteImageCache;

#[tokio::test]
async fn download_rejects_short_non_image_response() {
    let url = start_mock(vec![(200, vec![0u8; 50])]).await;
    let error = RemoteImageCache::for_test(direct_http()).fetch(&url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Unusable { detail } if detail.contains("valid image")),
        "got: {error}"
    );
}

#[tokio::test]
async fn download_rejects_bytes_that_are_not_an_image() {
    let url = start_mock(vec![(200, vec![b'x'; 256])]).await;
    let error = RemoteImageCache::for_test(direct_http()).fetch(&url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Unusable { detail } if detail.contains("valid image")),
        "got: {error}"
    );
}

#[tokio::test]
async fn download_rejects_declared_over_cap_response() {
    let url = start_declared_length_response(bae_util::http::MAX_IMAGE_BYTES + 1).await;
    let cache = RemoteImageCache::for_test(direct_http());
    let result =
        tokio::time::timeout(std::time::Duration::from_secs(1), cache.fetch_required(&url))
            .await
            .expect("oversized response should fail before reading the body");
    let error = result.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Unusable { detail } if detail.contains("too large")),
        "got: {error}"
    );
}

#[tokio::test]
async fn download_retries_transient_then_succeeds() {
    let body = test_png();
    let url = start_mock(vec![(503, vec![]), (200, body.clone())]).await;
    let image = RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap();
    assert_eq!(image.bytes, body);
}

#[tokio::test]
async fn artwork_http_failures_keep_their_status_after_retries() {
    for (status, attempts) in [(400, 1), (401, 1), (429, 4), (500, 4)] {
        let (host, url) = start_counting_host(status, Vec::new()).await;
        let error =
            RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap_err();
        assert_eq!(host.hits(), attempts, "status {status}");
        assert!(
            matches!(error, RemoteImageError::Status { status: answered, .. } if answered == status),
            "{error}"
        );
    }
}

#[tokio::test]
async fn artwork_interrupted_body_is_a_transport_failure() {
    let url = truncated_body_url().await;
    let error = RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Transport { error, .. } if error.is_decode()),
        "{error}"
    );
}

#[tokio::test]
async fn artwork_stalled_body_is_a_timeout() {
    let url = start_declared_length_response(256).await;
    let client =
        bae_util::http::client_builder().timeout(Duration::from_millis(30)).build().unwrap();
    let response = client.get(&url).send().await.unwrap();
    let error = match read_image_response(response).await {
        Err(error) => error,
        Ok(_) => panic!("stalled body should time out"),
    };
    assert!(
        matches!(&error, RemoteImageError::Transport { error, .. } if error.is_timeout()),
        "{error}"
    );
}

#[tokio::test]
async fn artwork_invalid_request_is_a_request_bae_could_not_build() {
    let error =
        RemoteImageCache::for_test(direct_http()).fetch_required("not a URL").await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Transport { error, .. } if error.is_builder()),
        "{error}"
    );
    assert!(error.to_string().contains("relative URL without a base"), "{error}");
}

/// An image request to a host no resolver knows names the DNS failure in its
/// text, where reqwest's own text says only that sending failed.
#[tokio::test]
async fn artwork_unresolvable_host_names_its_dns_failure() {
    let error = RemoteImageCache::for_test(direct_http())
        .fetch_required("http://bae-test.invalid/cover.jpg")
        .await
        .unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Transport { error, .. } if error.is_connect()),
        "{error}"
    );
    assert!(error.to_string().contains("dns error"), "{error}");
}

#[tokio::test]
async fn supported_cover_formats_keep_original_remote_bytes_and_type() {
    assert!(bae_util::test_cover_art::SOLID_GIF.len() < 100);
    assert!(bae_util::test_cover_art::SOLID_WEBP.len() < 100);
    for (bytes, content_type) in [
        (bae_util::test_cover_art::SOLID_GIF, ContentType::Gif),
        (bae_util::test_cover_art::SOLID_WEBP, ContentType::Webp),
        (bae_util::test_cover_art::ANIMATED_GIF, ContentType::Gif),
        (bae_util::test_cover_art::ANIMATED_WEBP, ContentType::Webp),
    ] {
        let (host, url) = start_counting_host(200, bytes.to_vec()).await;
        let cache = RemoteImageCache::for_test(direct_http());
        for _ in 0..2 {
            let image = cache.fetch_required(&url).await.unwrap();
            assert_eq!(image.bytes, bytes);
            assert_eq!(image.content_type, content_type);
        }
        assert_eq!(host.hits(), 1);
    }
}

#[tokio::test]
async fn remote_cover_dimensions_are_bounded() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(8193, 1).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let url = start_mock(vec![(200, bytes.into_inner())]).await;
    let error = RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Unusable { detail } if detail.contains("limit")),
        "{error}"
    );
}

#[tokio::test]
async fn remote_cover_decode_allocation_is_bounded() {
    let mut bytes = bae_util::test_cover_art::SOLID_GIF.to_vec();
    bytes[6..8].copy_from_slice(&6000u16.to_le_bytes());
    bytes[8..10].copy_from_slice(&6000u16.to_le_bytes());
    let url = start_mock(vec![(200, bytes)]).await;
    let error = RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap_err();
    assert!(
        matches!(&error, RemoteImageError::Unusable { detail } if detail.contains("Memory limit")),
        "{error}"
    );
}

#[tokio::test]
async fn truncated_gif_and_webp_fail_remote_validation_and_normalization() {
    for bytes in [bae_util::test_cover_art::SOLID_GIF, bae_util::test_cover_art::SOLID_WEBP] {
        let truncated = &bytes[..20];
        assert!(bae_util::stored_image::stored_jpeg(truncated).is_err());
        let url = start_mock(vec![(200, truncated.to_vec())]).await;
        let error =
            RemoteImageCache::for_test(direct_http()).fetch_required(&url).await.unwrap_err();
        assert!(matches!(error, RemoteImageError::Unusable { .. }));
    }
}
