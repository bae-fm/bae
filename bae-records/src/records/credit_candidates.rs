//! The library artists each of a write's credits can name, which the rule
//! (`artist_resolution`) decides between: one statement reads them for every
//! credit of the write, before the rule decides the first.

use super::catalog_match::{CatalogColumn, CatalogLibrary};
use super::read::ArtistJson;
use crate::DbArtist;
use bae_store::{statement_rows, DbError};
use std::convert::Infallible;

// One row for each credit of `credits`, with the artists it can name, each
// list in id order. `picked_held` says whether the library holds the artist
// `picked_id`. A list for a catalog id holds every artist whose column holds
// the id, or whose row the id names (its column may have been filled in or
// edited since), each followed through any merge to the artist it shows as.
// `named` holds every shown artist that goes by `name_key`, as its own name or
// as one of its other printed names; a printed name an absorbed artist held
// names the artist it shows as. No artist goes by an empty key.
sqlive_compose::fragment!(
    CreditCandidates with credits(
        position INTEGER, picked_id TEXT, name_key TEXT,
        musicbrainz_artist_id TEXT, discogs_artist_id TEXT,
        various_musicbrainz_artist_id TEXT, various_discogs_artist_id TEXT
    ) {
        position: i64,
        picked_held: bool,
        by_musicbrainz: json<Vec<crate::records::read::ArtistJson>>,
        by_discogs: json<Vec<crate::records::read::ArtistJson>>,
        by_various_musicbrainz: json<Vec<crate::records::read::ArtistJson>>,
        by_various_discogs: json<Vec<crate::records::read::ArtistJson>>,
        named: json<Vec<crate::records::read::ArtistJson>>
    },
    r#"
    WITH asked(found_by, lookup_key) AS MATERIALIZED (
        SELECT 'musicbrainz', musicbrainz_artist_id FROM credits
        WHERE musicbrainz_artist_id IS NOT NULL
        UNION SELECT 'musicbrainz', various_musicbrainz_artist_id FROM credits
              WHERE various_musicbrainz_artist_id IS NOT NULL
        UNION SELECT 'discogs', discogs_artist_id FROM credits
              WHERE discogs_artist_id IS NOT NULL
        UNION SELECT 'discogs', various_discogs_artist_id FROM credits
              WHERE various_discogs_artist_id IS NOT NULL
        UNION SELECT 'name', name_key FROM credits WHERE name_key <> ''
    ),
    held(found_by, lookup_key, named_id) AS (
        SELECT asked.found_by, asked.lookup_key, named.id
        FROM asked JOIN artists AS named
            ON asked.found_by = 'musicbrainz'
           AND named.musicbrainz_artist_id = asked.lookup_key
        UNION ALL
        SELECT asked.found_by, asked.lookup_key, named.id
        FROM asked JOIN artists AS named
            ON asked.found_by = 'discogs' AND named.discogs_artist_id = asked.lookup_key
        UNION ALL
        SELECT asked.found_by, asked.lookup_key, named.id
        FROM asked JOIN artists AS named
            ON named.id = CASE asked.found_by
                WHEN 'musicbrainz' THEN bae_artist_identity(asked.lookup_key, NULL)
                WHEN 'discogs' THEN bae_artist_identity(NULL, asked.lookup_key) END
    ),
    found(found_by, lookup_key, artist_id) AS (
        SELECT held.found_by, held.lookup_key, shown.shown_artist_id
        FROM held JOIN artist_shown_as AS shown ON shown.artist_id = held.named_id
        UNION
        SELECT asked.found_by, asked.lookup_key, shown.artist_id
        FROM asked
        JOIN artists AS named
            ON asked.found_by = 'name' AND named.name_key = asked.lookup_key
        JOIN artist_shown_as AS shown
            ON shown.artist_id = named.id AND shown.shown_artist_id = named.id
        UNION
        SELECT asked.found_by, asked.lookup_key, shown.shown_artist_id
        FROM asked
        JOIN artist_credits AS printed
            ON asked.found_by = 'name' AND printed.name_key = asked.lookup_key
        JOIN artist_shown_as AS shown ON shown.artist_id = printed.artist_id
    ),
    answered(found_by, lookup_key, artists) AS MATERIALIZED (
        SELECT found.found_by, found.lookup_key,
               json_group_array(json_object(
                   'id', artist.id, 'name', artist.name, 'sort_name', artist.sort_name,
                   'discogs_artist_id', artist.discogs_artist_id,
                   'musicbrainz_artist_id', artist.musicbrainz_artist_id,
                   'created_at', artist.created_at) ORDER BY artist.id)
        FROM found JOIN artists AS artist ON artist.id = found.artist_id
        GROUP BY found.found_by, found.lookup_key
    )
    SELECT credit.position,
           EXISTS (SELECT 1 FROM artists AS picked WHERE picked.id = credit.picked_id)
               AS picked_held,
           COALESCE(by_mb.artists, '[]') AS by_musicbrainz,
           COALESCE(by_dg.artists, '[]') AS by_discogs,
           COALESCE(various_mb.artists, '[]') AS by_various_musicbrainz,
           COALESCE(various_dg.artists, '[]') AS by_various_discogs,
           COALESCE(by_name.artists, '[]') AS named
    FROM credits AS credit
    LEFT JOIN answered AS by_mb
        ON by_mb.found_by = 'musicbrainz'
       AND by_mb.lookup_key = credit.musicbrainz_artist_id
    LEFT JOIN answered AS by_dg
        ON by_dg.found_by = 'discogs' AND by_dg.lookup_key = credit.discogs_artist_id
    LEFT JOIN answered AS various_mb
        ON various_mb.found_by = 'musicbrainz'
       AND various_mb.lookup_key = credit.various_musicbrainz_artist_id
    LEFT JOIN answered AS various_dg
        ON various_dg.found_by = 'discogs'
       AND various_dg.lookup_key = credit.various_discogs_artist_id
    LEFT JOIN answered AS by_name
        ON by_name.found_by = 'name' AND by_name.lookup_key = credit.name_key
"#
);

sqlive_compose::read!(
    CreditCandidatesOfCredits(credits: String),
    r#"
    SELECT candidate.*
    FROM @crate::records::credit_candidates::CreditCandidates(credits: (
        SELECT j.key AS position,
               j.value ->> 'picked_id' AS picked_id,
               j.value ->> 'name_key' AS name_key,
               j.value ->> 'musicbrainz_artist_id' AS musicbrainz_artist_id,
               j.value ->> 'discogs_artist_id' AS discogs_artist_id,
               j.value ->> 'various_musicbrainz_artist_id' AS various_musicbrainz_artist_id,
               j.value ->> 'various_discogs_artist_id' AS various_discogs_artist_id
        FROM json_each(:credits) AS j)) AS candidate
    ORDER BY candidate.position
"#
);

/// One credit a read looks up.
pub(super) struct LookedUpCredit<'a> {
    pub(super) credit: &'a DbArtist,
    /// Whether a person picked the library artist the credit's id names.
    pub(super) picked: bool,
    /// The credit going by both catalogs' Various Artists ids, where it
    /// names Various Artists by one of them.
    pub(super) various: Option<DbArtist>,
}

/// The library artists one credit can name, as the library held them when
/// they were read.
pub(super) struct Candidates {
    /// Whether the library holds the artist a person picked as the credit;
    /// false for a credit no one picked.
    pub(super) picked_held: bool,
    /// The artists holding the credit's catalog ids.
    pub(super) by_ids: IdHolders,
    /// The credit going by both catalogs' Various Artists ids, where it
    /// names Various Artists by one of them, and the artists holding those.
    pub(super) various: Option<(DbArtist, IdHolders)>,
    /// Every shown artist that goes by the credit's folded name, in id
    /// order.
    pub(super) named: Vec<DbArtist>,
}

/// The library artists holding each catalog id one credit carries.
pub(super) struct IdHolders {
    musicbrainz: Held,
    discogs: Held,
}

/// The library artists holding the catalog id `id`, in id order; none for a
/// catalog the credit carries no id of.
struct Held {
    id: Option<String>,
    artists: Vec<DbArtist>,
}

impl CatalogLibrary<DbArtist> for IdHolders {
    type Error = Infallible;

    /// The catalog-id step asks only for the ids of the credit it resolves,
    /// so another id is a bug of the step's, which panics.
    fn holding(
        &self,
        column: CatalogColumn,
        catalog_id: &str,
    ) -> Result<Vec<DbArtist>, Infallible> {
        let held = match column {
            CatalogColumn::Discogs => &self.discogs,
            CatalogColumn::MusicBrainz => &self.musicbrainz,
        };
        assert_eq!(held.id.as_deref(), Some(catalog_id), "an id the credit was not read by");
        Ok(held.artists.clone())
    }
}

/// Read the library artists each of `credits` can name, positionally.
pub(super) fn candidates_on<Q: coven::SqlRead>(
    sql: &Q,
    credits: Vec<LookedUpCredit<'_>>,
) -> Result<Vec<Candidates>, DbError> {
    let request = CreditCandidatesOfCreditsParams {
        credits: serde_json::Value::Array(credits.iter().map(lookup_json).collect()).to_string(),
    };
    // The statement returns one row for each credit, in their order.
    statement_rows(sql, &request)?
        .into_iter()
        .zip(credits)
        .map(|(row, looked_up)| candidates_of_row(row.candidate, looked_up))
        .collect()
}

fn lookup_json(looked_up: &LookedUpCredit<'_>) -> serde_json::Value {
    let credit = looked_up.credit;
    let various = looked_up.various.as_ref();
    serde_json::json!({
        "picked_id": looked_up.picked.then_some(&credit.id),
        "name_key": bae_types::text_match::artist_name_key(&credit.name),
        "musicbrainz_artist_id": credit.musicbrainz_artist_id,
        "discogs_artist_id": credit.discogs_artist_id,
        "various_musicbrainz_artist_id":
            various.and_then(|various| various.musicbrainz_artist_id.as_ref()),
        "various_discogs_artist_id":
            various.and_then(|various| various.discogs_artist_id.as_ref()),
    })
}

fn candidates_of_row(
    row: CreditCandidates,
    looked_up: LookedUpCredit<'_>,
) -> Result<Candidates, DbError> {
    let by_ids = id_holders(looked_up.credit, row.by_musicbrainz, row.by_discogs)?;
    let various = match looked_up.various {
        Some(various) => {
            let holders = id_holders(&various, row.by_various_musicbrainz, row.by_various_discogs)?;
            Some((various, holders))
        }
        None => None,
    };
    Ok(Candidates { picked_held: row.picked_held, by_ids, various, named: artists_of(row.named)? })
}

/// The artists holding each of `credit`'s catalog ids, from the lists the
/// statement returned for them.
fn id_holders(
    credit: &DbArtist,
    musicbrainz: Vec<ArtistJson>,
    discogs: Vec<ArtistJson>,
) -> Result<IdHolders, DbError> {
    Ok(IdHolders {
        musicbrainz: Held {
            id: credit.musicbrainz_artist_id.clone(),
            artists: artists_of(musicbrainz)?,
        },
        discogs: Held { id: credit.discogs_artist_id.clone(), artists: artists_of(discogs)? },
    })
}

fn artists_of(artists: Vec<ArtistJson>) -> Result<Vec<DbArtist>, DbError> {
    artists.into_iter().map(ArtistJson::into_artist).collect()
}
