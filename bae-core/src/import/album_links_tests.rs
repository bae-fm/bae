use super::*;

const GROUP: &str = "mb-group";

fn musicbrainz(release_id: &str) -> MetadataResult {
    MetadataResult::for_test(Catalog::MusicBrainz, release_id, Some(GROUP))
}

fn discogs_release(release_id: &str, master: &str) -> MetadataResult {
    MetadataResult::for_test(Catalog::Discogs, release_id, Some(master))
}

fn names(master: &str, stated: AlbumStatement) -> AlbumLink {
    AlbumLink {
        album: MetadataRef::new(Catalog::Discogs, master),
        stated,
    }
}

/// `list` read with `stated` answering for the releases it names.
fn read(list: &[MetadataResult], stated: &[(&str, AlbumLinks)]) -> Vec<GroupLinks> {
    read_groups(&list.iter().collect::<Vec<_>>(), |release| {
        stated
            .iter()
            .find(|(read, _)| *read == release)
            .map(|(_, links)| links.clone())
    })
}

/// Every album the group's read releases name is the group's, each once.
#[test]
fn a_group_is_every_album_its_read_releases_name() {
    let list = [musicbrainz("mb-1"), musicbrainz("mb-2")];
    let groups = read(
        &list,
        &[
            ("mb-1", AlbumLinks::Read(vec![names("7", AlbumStatement::Page)])),
            (
                "mb-2",
                AlbumLinks::Read(vec![
                    names("7", AlbumStatement::Page),
                    names("8", AlbumStatement::Page),
                ]),
            ),
        ],
    );
    assert_eq!(
        groups,
        vec![GroupLinks {
            group: GROUP.to_string(),
            links: AlbumLinks::Read(vec![
                names("7", AlbumStatement::Page),
                names("8", AlbumStatement::Page),
            ]),
        }]
    );
}

/// A named album stands over a release whose documents could not be had; with
/// none named, the group is unknown, and the list is not read for it.
#[test]
fn an_unread_release_leaves_the_group_unknown_only_where_nothing_is_named() {
    let mut printed = discogs_release("dg-1", "7");
    printed.barcodes = vec!["012345678905".to_string()];
    let mut ours = musicbrainz("mb-1");
    ours.barcodes = printed.barcodes.clone();
    let list = [ours, musicbrainz("mb-2"), printed];

    let named = read(
        &list,
        &[
            ("mb-1", AlbumLinks::Read(vec![names("7", AlbumStatement::Page)])),
            ("mb-2", AlbumLinks::Unread),
        ],
    );
    assert_eq!(
        named[0].links,
        AlbumLinks::Read(vec![names("7", AlbumStatement::Page)])
    );

    let unknown = read(
        &list,
        &[("mb-1", AlbumLinks::Read(Vec::new())), ("mb-2", AlbumLinks::Unread)],
    );
    assert_eq!(unknown[0].links, AlbumLinks::Unread);
}

/// A group whose read documents name nothing is what the list's releases
/// print: here one barcode.
#[test]
fn a_group_whose_documents_name_nothing_is_what_the_list_prints() {
    let mut ours = musicbrainz("mb-1");
    ours.barcodes = vec!["012345678905".to_string()];
    let mut theirs = discogs_release("dg-1", "7");
    theirs.barcodes = ours.barcodes.clone();
    let groups = read(&[ours, theirs], &[("mb-1", AlbumLinks::Read(Vec::new()))]);
    assert_eq!(
        groups[0].links,
        AlbumLinks::Read(vec![names(
            "7",
            AlbumStatement::Barcode {
                musicbrainz_release: "mb-1".to_string(),
                release: MetadataRef::new(Catalog::Discogs, "dg-1"),
            },
        )])
    );
}

/// A group none of whose releases was read is not read at all, even where
/// what the list prints would join it.
#[test]
fn a_group_nothing_read_is_not_read() {
    let mut ours = musicbrainz("mb-1");
    ours.barcodes = vec!["012345678905".to_string()];
    let mut theirs = discogs_release("dg-1", "7");
    theirs.barcodes = ours.barcodes.clone();
    assert!(read(&[ours, theirs], &[]).is_empty());
}

/// What a group was read to be goes onto its MusicBrainz records only.
#[test]
fn a_group_s_links_go_onto_its_musicbrainz_records() {
    let groups = vec![GroupLinks {
        group: GROUP.to_string(),
        links: AlbumLinks::Unread,
    }];
    let mut ours = musicbrainz("mb-1");
    let mut other = MetadataResult::for_test(Catalog::MusicBrainz, "mb-9", Some("other-group"));
    let mut theirs = discogs_release("dg-1", GROUP);
    for result in [&mut ours, &mut other, &mut theirs] {
        apply(result, &groups);
    }
    assert_eq!(ours.album_links, AlbumLinks::Unread);
    assert_eq!(other.album_links, AlbumLinks::NotAsked);
    assert_eq!(theirs.album_links, AlbumLinks::NotAsked);
}

/// A group naming an album is kept; one naming none only where the list held
/// another catalog's album to compare it against; an unknown one never.
#[test]
fn what_is_kept_is_what_the_list_could_say() {
    let named = GroupLinks {
        group: "g-named".to_string(),
        links: AlbumLinks::Read(vec![names("7", AlbumStatement::Page)]),
    };
    let nothing = GroupLinks {
        group: "g-nothing".to_string(),
        links: AlbumLinks::Read(Vec::new()),
    };
    let unknown = GroupLinks {
        group: "g-unknown".to_string(),
        links: AlbumLinks::Unread,
    };
    let groups = [named.clone(), nothing, unknown];

    let alone = [musicbrainz("mb-1")];
    assert_eq!(
        to_keep(&groups, &alone.iter().collect::<Vec<_>>()),
        vec![("g-named".to_string(), named.links.read().to_vec())]
    );

    let beside = [musicbrainz("mb-1"), discogs_release("dg-1", "7")];
    assert_eq!(
        to_keep(&groups, &beside.iter().collect::<Vec<_>>()),
        vec![
            ("g-named".to_string(), named.links.read().to_vec()),
            ("g-nothing".to_string(), Vec::new()),
        ]
    );
}
