use super::ShownImageTypesParams;
use bae_util::content_type::ContentType;
use coven::{Returning, Statement};
use std::collections::BTreeSet;

/// The images bae can show are the formats a cover can be made from.
#[test]
fn the_showable_image_types_are_the_cover_formats() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let mut statement = connection.prepare(ShownImageTypesParams::SQL).unwrap();
    let shown: BTreeSet<String> = statement
        .query_map([], |row| Ok(ShownImageTypesParams::read(row)?.shown.content_type))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let covers: BTreeSet<String> = ContentType::SUPPORTED_COVERS
        .iter()
        .map(|content_type| content_type.as_str().to_string())
        .collect();
    assert_eq!(shown, covers);
}
