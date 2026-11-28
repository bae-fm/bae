use super::ShownImageTypesParams;
use bae_util::content_type::ContentType;
use std::collections::BTreeSet;

/// The images bae can show are the formats a cover can be made from.
#[tokio::test]
async fn the_showable_image_types_are_the_cover_formats() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::test_fixtures::TEST_SCHEMA).await;
    let rows = db.read_rows(ShownImageTypesParams {}).await.unwrap();
    let shown: BTreeSet<String> = rows.into_iter().map(|row| row.shown.content_type).collect();
    let covers: BTreeSet<String> = ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| content_type.as_str().to_string())
        .collect();
    assert_eq!(shown, covers);
}
