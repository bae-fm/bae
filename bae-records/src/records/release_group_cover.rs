//! The SQL function that states the cover of a MusicBrainz release group.

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// `bae_release_group_cover`: the Cover Art Archive's front image of the
/// MusicBrainz release group named by its argument, as the JSON object of the
/// columns a stored cover holds (`url`, `label`, `source`, `standing`,
/// `copies`). One definition of where that image lives, for statements that
/// offer the cover of an album a kept statement joins to a release.
pub(crate) static RELEASE_GROUP_COVER: SchemaFunction = SchemaFunction {
    name: "bae_release_group_cover",
    arguments: 1,
    version: 1,
    compute: |arguments| {
        let group = arguments.get::<String>(0)?;
        let cover = bae_catalogs::remote_image::RemoteCover::musicbrainz_release_group(&group);
        let copies: Vec<serde_json::Value> = cover
            .image
            .downscaled
            .iter()
            .map(|copy| serde_json::json!({ "url": copy.url, "max_edge": copy.max_edge }))
            .collect();
        let object = serde_json::json!({
            "url": cover.image.url,
            "label": cover.label,
            "source": cover.source.as_str(),
            "standing": cover.standing.as_str(),
            "copies": copies,
        });
        Ok(Value::Text(object.to_string()))
    },
};
