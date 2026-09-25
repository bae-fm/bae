use super::*;
use crate::import::ParsedAlbum;
use coven::{FixedClock, SequentialIdProvider};
use serde_json::json;

fn selected(urls: &[&str]) -> ReleasePayloads {
    ReleasePayloads {
        release: MetadataRef::new(Catalog::MusicBrainz, "selected-release"),
        anchor: json!({
            "id":"selected-release", "title":"Selected Album", "date":"2005",
            "artist-credit":[{"name":"Selected Artist","artist":{"id":"artist-id","name":"Selected Artist"}}],
            "media":[{"format":"Vinyl","tracks":[{"number":"A1","title":"First Track"},{"number":"B1","title":"Second Track"}]}],
            "relations":urls.iter().map(|url| json!({"url":{"resource":url}})).collect::<Vec<_>>(),
            "cover-art-archive":{"front":false,"darkened":false}
        }).to_string(),
        supporting: vec![],
    }
}

fn pressing(id: u64, parent: Option<u64>) -> SourcePayload {
    SourcePayload::new(
        PayloadSource::Discogs,
        id.to_string(),
        json!({
            "id":id,"title":"Linked Album","master_id":parent,
        })
        .to_string(),
    )
}

fn master(id: u64, year: u32) -> SourcePayload {
    SourcePayload::new(
        PayloadSource::DiscogsMaster,
        id.to_string(),
        json!({
            "id":id,"title":"Parent Album","year":year,
            "images":[{"type":"primary","uri":format!("https://images.example/{id}.jpg")}]
        })
        .to_string(),
    )
}

fn parsed(payloads: &ReleasePayloads) -> ParsedAlbum {
    let now = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    payloads
        .extract()
        .unwrap()
        .parsed(
            &[],
            &FixedClock(now),
            &SequentialIdProvider::new("ambiguity"),
        )
        .unwrap()
}

fn assert_permutation(payloads: &ReleasePayloads, check: impl Fn(&ReleasePayloads)) {
    check(payloads);
    let mut reversed = payloads.clone();
    reversed.supporting.reverse();
    for document in &mut reversed.supporting {
        let mut json: serde_json::Value = serde_json::from_str(&document.json).unwrap();
        if let Some(relations) = json
            .get_mut("relations")
            .and_then(serde_json::Value::as_array_mut)
        {
            relations.reverse();
        }
        if let Some(entities) = json
            .get_mut("entities")
            .and_then(serde_json::Value::as_object_mut)
        {
            for entity in entities.values_mut() {
                if let Some(claims) = entity
                    .get_mut("claims")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    for statements in claims.values_mut() {
                        statements.as_array_mut().unwrap().reverse();
                    }
                }
            }
        }
        document.json = json.to_string();
    }
    let mut anchor: serde_json::Value = serde_json::from_str(&reversed.anchor).unwrap();
    anchor["relations"].as_array_mut().unwrap().reverse();
    reversed.anchor = anchor.to_string();
    check(&reversed);
    assert_eq!(payloads.extract().unwrap().records(), reversed.extract().unwrap().records());
    assert_eq!(parsed(payloads).album.year, parsed(&reversed).album.year);
}

#[test]
fn ambiguous_pressings_contribute_only_a_proven_common_parent() {
    for parents in [
        (Some(101), Some(102)),
        (Some(101), None),
        (Some(101), Some(101)),
    ] {
        let mut payloads = selected(&[
            "https://www.discogs.com/release/11",
            "https://www.discogs.com/release/12",
        ]);
        payloads.supporting = vec![
            pressing(11, parents.0),
            pressing(12, parents.1),
            master(101, 1971),
            master(102, 1982),
        ];
        assert_permutation(&payloads, |payloads| {
            let same = parents == (Some(101), Some(101));
            assert_eq!(
                parsed(payloads).album.year,
                Some(if same { 1971 } else { 2005 })
            );
            let records = payloads.extract().unwrap().records();
            let discogs = records
                .iter()
                .find(|record| record.catalog() == Catalog::Discogs);
            assert_eq!(
                discogs.map(|record| record.url()),
                same.then(|| "https://www.discogs.com/master/101".to_string())
            );
            assert!(discogs.is_none_or(|record| record.release_ref().is_none()));
            assert_eq!(payloads.extract().unwrap().covers().len(), usize::from(same));
            let parsed = parsed(payloads);
            assert_eq!(parsed.tracks[1].side, Some(2));
            assert_eq!(parsed.tracks[1].title, "Second Track");
        });
    }
}

#[test]
fn missing_ambiguous_pressing_does_not_prove_a_common_parent() {
    let mut payloads = selected(&[
        "https://www.discogs.com/release/11",
        "https://www.discogs.com/release/12",
    ]);
    payloads.supporting = vec![pressing(11, Some(101)), master(101, 1971)];
    assert_permutation(&payloads, |payloads| {
        assert_eq!(parsed(payloads).album.year, Some(2005));
        assert_eq!(payloads.extract().unwrap().records().len(), 1);
        assert!(payloads.extract().unwrap().covers().is_empty());
    });
}

#[test]
fn competing_direct_album_links_do_not_choose_by_key_or_order() {
    let mut payloads = selected(&[
        "https://www.discogs.com/master/101",
        "https://www.discogs.com/master/102",
    ]);
    payloads.supporting = vec![master(101, 1971), master(102, 1982)];
    assert_permutation(&payloads, |payloads| {
        assert_eq!(parsed(payloads).album.year, Some(2005));
        assert_eq!(payloads.extract().unwrap().records().len(), 1);
        assert!(payloads.extract().unwrap().covers().is_empty());
    });
}

#[test]
fn competing_direct_catalog_claims_are_not_resolved_by_weaker_wikidata_claims() {
    let mut payloads = selected(&[
        "https://www.allmusic.com/album/mw11",
        "https://www.allmusic.com/album/mw12",
        "https://www.wikidata.org/wiki/Q101",
    ]);
    payloads.supporting.push(SourcePayload::new(
        PayloadSource::Wikidata,
        "Q101",
        json!({
            "entities":{"Q101":{"claims":{"P1729":[{"mainsnak":{"datavalue":{"value":"mw11"}}}]}}}
        })
        .to_string(),
    ));
    assert_permutation(&payloads, |payloads| {
        assert!(!payloads
            .extract().unwrap()
            .records()
            .iter()
            .any(|record| record.catalog() == Catalog::AllMusic));
    });
}

#[test]
fn competing_wikidata_values_are_unclaimed_and_stronger_direct_identity_wins() {
    for direct in [false, true] {
        let mut urls = vec!["https://www.wikidata.org/wiki/Q101"];
        if direct {
            urls.push("https://www.allmusic.com/album/mw13");
        }
        let mut payloads = selected(&urls);
        payloads.supporting.push(SourcePayload::new(
            PayloadSource::Wikidata,
            "Q101",
            json!({
                "entities":{"Q101":{"claims":{"P1729":[
                    {"mainsnak":{"datavalue":{"value":"mw11"}}},
                    {"mainsnak":{"datavalue":{"value":"mw12"}}}
                ]}}}
            })
            .to_string(),
        ));
        assert_permutation(&payloads, |payloads| {
            let records = payloads.extract().unwrap().records();
            assert_eq!(
                records
                    .iter()
                    .find(|record| record.catalog() == Catalog::AllMusic)
                    .map(|record| record.url()),
                direct.then(|| "https://www.allmusic.com/album/mw13".to_string())
            );
        });
    }
}

#[test]
fn an_admitted_wikidata_item_cannot_restore_a_conflicting_album() {
    let mut payloads = selected(&[
        "https://www.discogs.com/master/101",
        "https://www.discogs.com/master/102",
        "https://www.wikidata.org/wiki/Q101",
    ]);
    payloads.supporting = vec![master(101, 1971), master(102, 1982), SourcePayload::new(
        PayloadSource::Wikidata, "Q101", json!({
            "entities":{"Q101":{"claims":{"P1954":[{"mainsnak":{"datavalue":{"value":"101"}}}]}}}
        }).to_string(),
    )];
    assert_permutation(&payloads, |payloads| {
        assert_eq!(parsed(payloads).album.year, Some(2005));
        assert!(!payloads
            .extract().unwrap()
            .records()
            .iter()
            .any(|record| record.catalog() == Catalog::Discogs));
        assert!(payloads.extract().unwrap().covers().is_empty());
    });
}

#[test]
fn selected_parent_wins_over_conflicting_album_links() {
    let mut payloads = selected(&[
        "https://musicbrainz.org/release-group/other-group",
        "https://musicbrainz.org/release-group/another-group",
    ]);
    let mut anchor: serde_json::Value = serde_json::from_str(&payloads.anchor).unwrap();
    anchor["release-group"] = json!({"id":"selected-group"});
    payloads.anchor = anchor.to_string();
    for (id, year) in [
        ("selected-group", "1980"),
        ("other-group", "1971"),
        ("another-group", "1972"),
    ] {
        payloads.supporting.push(SourcePayload::new(
            PayloadSource::MusicBrainzReleaseGroup,
            id,
            json!({"id":id,"first-release-date":year}).to_string(),
        ));
    }
    assert_permutation(&payloads, |payloads| {
        assert_eq!(parsed(payloads).album.year, Some(1980));
        assert_eq!(
            payloads.extract().unwrap().records()[0].album_ref().unwrap().key,
            "selected-group"
        );
    });
}

#[test]
fn selected_album_url_precedes_counterpart_parent_inference() {
    for counterpart_urls in [
        vec!["https://www.discogs.com/release/11"],
        vec![
            "https://www.discogs.com/release/11",
            "https://www.discogs.com/release/12",
        ],
    ] {
        let mut urls = counterpart_urls;
        urls.push("https://www.discogs.com/master/102");
        let mut payloads = selected(&urls);
        payloads.supporting = vec![
            pressing(11, Some(101)),
            pressing(12, Some(101)),
            master(101, 1971),
            master(102, 1982),
        ];
        assert_permutation(&payloads, |payloads| {
            assert_eq!(parsed(payloads).album.year, Some(1982));
            assert_eq!(
                payloads.album_links().unwrap(),
                vec![MetadataRef::new(Catalog::Discogs, "102")]
            );
            let records = payloads.extract().unwrap().records();
            let discogs = records
                .iter()
                .find(|record| record.catalog() == Catalog::Discogs)
                .unwrap();
            if let Some(release) = discogs.release_ref() {
                assert_eq!(release.key, "11");
                assert_eq!(
                    discogs.album_ref().unwrap().key,
                    "101",
                    "metadata precedence must not rewrite a pressing's stated parent"
                );
            } else {
                assert_eq!(discogs.album_ref().unwrap().key, "102");
            }
        });
    }
}

#[test]
fn conflicting_counterpart_parents_block_weaker_wikidata_parent() {
    for missing in [false, true] {
        let mut payloads = selected(&[
            "https://www.discogs.com/release/11",
            "https://www.discogs.com/release/12",
            "https://www.wikidata.org/wiki/Q101",
        ]);
        if missing {
            let mut anchor: serde_json::Value = serde_json::from_str(&payloads.anchor).unwrap();
            anchor["relations"]
                .as_array_mut()
                .unwrap()
                .push(json!({"url":{"resource":"https://www.discogs.com/release/13"}}));
            payloads.anchor = anchor.to_string();
        }
        payloads.supporting = vec![pressing(11, Some(101)), pressing(12, Some(102)), master(101, 1971), master(102, 1982), SourcePayload::new(PayloadSource::Wikidata, "Q101", json!({"entities":{"Q101":{"claims":{"P1954":[{"mainsnak":{"datavalue":{"value":"101"}}}]}}}}).to_string())];
        assert_permutation(&payloads, |payloads| {
            assert_eq!(parsed(payloads).album.year, Some(2005));
            assert!(!payloads
                .extract().unwrap()
                .records()
                .iter()
                .any(|record| record.catalog() == Catalog::Discogs));
            assert!(payloads.extract().unwrap().covers().is_empty());
        });
    }
}
