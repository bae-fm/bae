//! What the catalog image cache offers the reads of other crates: the size of
//! each file it keeps.

coven::fragment!(
    CachedImageBytes,
    r#"
    SELECT image.bytes
    FROM cached_image AS image
"#
);
