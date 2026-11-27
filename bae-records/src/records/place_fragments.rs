//! Where tracks sit on mediums, as fragments over rows the caller gives: the
//! library's mediums and tracks, the rows a release editor's form lays out,
//! or an import's draft. [`super::release_tracks::track_position_of`] reads a
//! placed track's kind, ordinal and number, and
//! [`crate::DbSheetSideColumns`] a side choice.

// Each of the mediums the caller gives, with whether it is played side by
// side, how many sided sides come before it on its release, and how many
// mediums its release holds. A record, a cassette and every other carrier
// `bae_types::pressing::Medium::is_sided` names is played side by side.
sqlive_compose::fragment!(
    PlacedMediums
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ) {
        medium_id: String,
        release_id: String,
        position: i64,
        medium: Option<String>,
        side_count: i64,
        sided: bool,
        sides_before: i64,
        medium_count: i64
    },
    r#"
    SELECT m.id AS medium_id, m.release_id, m.position, m.medium, m.side_count,
           COALESCE(m.medium IN ('vinyl', 'shellac', 'acetate', 'cassette', 'dcc',
                                 'edison_disc', 'pathe_disc'), 0) AS sided,
           COALESCE(SUM(CASE WHEN COALESCE(m.medium IN ('vinyl', 'shellac', 'acetate',
                                 'cassette', 'dcc', 'edison_disc', 'pathe_disc'), 0)
                             THEN m.side_count ELSE 0 END)
                    OVER (PARTITION BY m.release_id ORDER BY m.position
                          ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0)
               AS sides_before,
           COUNT(*) OVER (PARTITION BY m.release_id) AS medium_count
    FROM mediums AS m
"#
);

// Each of the tracks the caller gives on the mediums it gives, a track on none
// of them left out. A track on a medium played side by side is on a lettered
// side, the letters counting the sides of its release's sided mediums in
// order (`side_kind` `sided`); a track on another medium is on a numbered
// disc where its release holds more than one medium (`disc`), and only a
// number otherwise (`flat`). `side_ordinal` is the side's number among the
// sided sides, the disc's number, or zero. `position` is the track's playing
// order, `side` its side on its medium, counted from one, `number` its number
// on that side, and `carrier` its medium's carrier key where the source
// states one.
sqlive_compose::fragment!(
    TrackPlaces
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ), places(
        id TEXT, release_id TEXT, medium_id TEXT, side INTEGER, track_number INTEGER,
        stated_length_ms INTEGER, position INTEGER
    ) {
        track_id: String,
        release_id: String,
        position: i64,
        medium_id: String,
        side: i64,
        number: i64,
        stated_length_ms: Option<i64>,
        medium_position: i64,
        carrier: Option<String>,
        side_kind: String,
        side_ordinal: i64
    },
    r#"
    WITH medium AS (
        SELECT placed.*
        FROM @crate::records::place_fragments::PlacedMediums(
            mediums: (SELECT * FROM mediums)) AS placed)
    SELECT p.id AS track_id, p.release_id, p.position, p.medium_id, p.side,
           p.track_number AS number, p.stated_length_ms, m.position AS medium_position,
           m.medium AS carrier,
           CASE WHEN m.sided THEN 'sided'
                WHEN m.medium_count > 1 THEN 'disc'
                ELSE 'flat' END AS side_kind,
           CASE WHEN m.sided THEN m.sides_before + p.side
                WHEN m.medium_count > 1 THEN m.position
                ELSE 0 END AS side_ordinal
    FROM places AS p
    JOIN medium AS m ON m.medium_id = p.medium_id AND m.release_id = p.release_id
"#
);

// Every side or disc of the mediums of one release the caller gives that a
// CUE sheet can be put on: each side of a medium played side by side,
// lettered as the release's tracks are, and the one side of every other
// medium, numbered as its disc. `medium` is the medium's position, `side` the
// side on it, counted from one, and `kind` and `ordinal` the heading's. A
// join over every count of sides a medium can have gives each medium its
// sides: a recursive count would hide from coven which rows a statement
// reads.
sqlive_compose::fragment!(
    SheetSideChoices
    with mediums(
        id TEXT, release_id TEXT, position INTEGER, medium TEXT, side_count INTEGER
    ) {
        medium: i64,
        side: Option<i64>,
        kind: String,
        ordinal: i64
    },
    r#"
    WITH medium AS (
        SELECT placed.*
        FROM @crate::records::place_fragments::PlacedMediums(
            mediums: (SELECT * FROM mediums)) AS placed)
    SELECT m.position AS medium, counted.value AS side,
           CASE WHEN m.sided THEN 'sided' ELSE 'disc' END AS kind,
           CASE WHEN m.sided THEN m.sides_before + counted.value ELSE m.position END AS ordinal
    FROM medium AS m
    JOIN json_each('[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150,151,152,153,154,155,156,157,158,159,160,161,162,163,164,165,166,167,168,169,170,171,172,173,174,175,176,177,178,179,180,181,182,183,184,185,186,187,188,189,190,191,192,193,194,195,196,197,198,199,200,201,202,203,204,205,206,207,208,209,210,211,212,213,214,215,216,217,218,219,220,221,222,223,224,225,226,227,228,229,230,231,232,233,234,235,236,237,238,239,240,241,242,243,244,245,246,247,248,249,250,251,252,253,254,255]') AS counted
      ON counted.value = 1 OR (m.sided AND counted.value <= m.side_count)
"#
);
