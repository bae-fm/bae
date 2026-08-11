use super::*;

/// Run the real catalog extractor over plain lines, projected to bare values
/// — so these assertions exercise the regexes, ZIP rejection, and dedup
/// through the same entry point production uses.
fn cats(lines: &[String]) -> Vec<String> {
    let sourced: Vec<SourcedLine> = lines
        .iter()
        .map(|t| SourcedLine {
            source: Source::CueField,
            text: t.clone(),
        })
        .collect();
    catalog_numbers_sourced(&sourced)
        .into_iter()
        .map(|c| c.value)
        .collect()
}

/// Lines that survive `should_reject_line` — the same predicate production
/// applies before clustering.
fn kept_lines(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|l| !should_reject_line(l))
        .cloned()
        .collect()
}

// MARK: - catalog_numbers — preserve separators verbatim

#[test]
fn catalogs_preserves_hyphen() {
    assert_eq!(
        cats(&["WPCR-80001".to_string()]),
        vec!["WPCR-80001".to_string()],
    );
}

#[test]
fn catalogs_preserves_inner_space() {
    assert_eq!(
        cats(&["COCQ 84487".to_string()]),
        vec!["COCQ 84487".to_string()],
    );
}

#[test]
fn catalogs_substring_match() {
    assert_eq!(
        cats(&["Part No. TOCP12345".to_string()]),
        vec!["TOCP12345".to_string()],
    );
}

#[test]
fn catalogs_lowercase_rejected() {
    // The regexes require uppercase letters.
    let empty: Vec<String> = Vec::new();
    assert_eq!(cats(&["lowercase-12345".to_string()]), empty);
}

#[test]
fn catalogs_separator_variants_are_distinct() {
    assert_eq!(
        cats(&["WPCR-80001".to_string(), "WPCR 80001".to_string()]),
        vec!["WPCR-80001".to_string(), "WPCR 80001".to_string()],
    );
}

// MARK: - catalog_numbers — multi-disc suffix behavior
//
// The regex's `\b` word boundaries end a match at the first non-word
// character, so a multi-disc suffix after `/` or `~` falls outside it and
// only the first disc's number comes back. A format with two internal
// separators (`UDCD-1-702`) doesn't match at all — the regex permits one
// separator between letters and digits.

#[test]
fn catalogs_multi_disc_slash_suffix() {
    assert_eq!(
        cats(&["BVCK-15024/5".to_string()]),
        vec!["BVCK-15024".to_string()],
    );
}

#[test]
fn catalogs_multi_disc_tilde_suffix() {
    assert_eq!(
        cats(&["BVCP 21011~2".to_string()]),
        vec!["BVCP 21011".to_string()],
    );
}

#[test]
fn catalogs_two_internal_separators_rejected() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(cats(&["MFSL UDCD-1-702".to_string()]), empty);
}

// MARK: - catalog_numbers_sourced — origins + parity with catalog_numbers

#[test]
fn sourced_catalogs_attribute_origin_per_line() {
    // Folder brackets bypass this path, so a path component and an artwork
    // line stand in. Each survivor carries its source's `SignalOrigin`.
    let lines = vec![
        SourcedLine {
            source: Source::PathComponent,
            text: "WPCR-80001".to_string(),
        },
        SourcedLine {
            source: Source::Artwork(PathBuf::from("/cover.jpg")),
            text: "COCQ 84487".to_string(),
        },
    ];
    let out = catalog_numbers_sourced(&lines);
    assert_eq!(
        out,
        vec![
            SourcedValue::new("WPCR-80001".to_string(), SignalOrigin::FolderName),
            SourcedValue::new("COCQ 84487".to_string(), SignalOrigin::Artwork),
        ],
    );
}

#[test]
fn sourced_catalogs_reject_zip_and_dedup() {
    let raw = vec![
        "WPCR-80001".to_string(),
        "Some City, NY 10001".to_string(), // ZIP tail — rejected
        "WPCR-80001".to_string(),          // duplicate — dropped
        "Z1 12345".to_string(),
    ];
    assert_eq!(
        cats(&raw),
        vec!["WPCR-80001".to_string(), "Z1 12345".to_string()]
    );
}

// MARK: - free_text

#[test]
fn free_text_drops_catalog_lines() {
    assert_eq!(
        kept_lines(&["Album Title".to_string(), "WPCR-80001".to_string()]),
        vec!["Album Title".to_string()],
    );
}

#[test]
fn free_text_keeps_substring_catalog_lines() {
    // Only a *whole-line* catalog is rejected.
    assert_eq!(
        kept_lines(&["Label Records - WPCR-80001".to_string()]),
        vec!["Label Records - WPCR-80001".to_string()],
    );
}

// MARK: - sanity

#[test]
fn catalogs_empty_input() {
    let empty: Vec<String> = Vec::new();
    assert_eq!(cats(&[]), empty);
}

// MARK: - extract_folder_brackets

#[test]
fn brackets_accepts_catalog_shapes() {
    let folder = "Album Title [XX34b] (LABELA071) [Z1 12345]";
    let out = extract_folder_brackets(folder);
    assert_eq!(
        out,
        vec![
            "XX34b".to_string(),
            "LABELA071".to_string(),
            "Z1 12345".to_string(),
        ],
    );
}

#[test]
fn brackets_rejects_format_tags() {
    // Fewer than 2 digits.
    let folder = "Album Title [Vinyl] (Deluxe Edition) [2 CD]";
    let out = extract_folder_brackets(folder);
    assert!(out.is_empty(), "expected empty, got {out:?}");
}

#[test]
fn brackets_rejects_bare_year() {
    // No letters.
    let folder = "Album Title (1990)";
    assert!(extract_folder_brackets(folder).is_empty());
}

#[test]
fn brackets_rejects_over_length() {
    // The 20-char ceiling keeps quotations and liner-note fragments out.
    let folder = "Album [This is way too long to be a catalog]";
    assert!(extract_folder_brackets(folder).is_empty());
}

// MARK: - strip_path_component

#[test]
fn path_strips_year_and_bracket() {
    assert_eq!(
        strip_path_component("1989 - Album Title [XX34b]"),
        Some("Album Title".to_string()),
    );
}

#[test]
fn path_strips_dotted_year() {
    assert_eq!(
        strip_path_component("1989. Album Title"),
        Some("Album Title".to_string()),
    );
}

#[test]
fn path_strips_trailing_brackets_multiple() {
    assert_eq!(
        strip_path_component("Album Title [Deluxe] (2020)"),
        Some("Album Title".to_string()),
    );
}

#[test]
fn path_leaves_leading_bracket_alone() {
    // TRAILING_BRACKET is anchored at end-of-line, so a leading bracket
    // stays attached to the rest of the component.
    assert_eq!(
        strip_path_component("[Remaster] Album Title"),
        Some("[Remaster] Album Title".to_string()),
    );
}

#[test]
fn path_iteratively_strips_multiple_trailing_brackets() {
    assert_eq!(
        strip_path_component("Album Title [Remaster] [Deluxe]"),
        Some("Album Title".to_string()),
    );
}

#[test]
fn path_returns_none_when_empty() {
    assert_eq!(strip_path_component("[XX34b]"), None);
    assert_eq!(strip_path_component("1989"), None);
}

#[test]
fn path_min_length_gate() {
    assert_eq!(strip_path_component("1989 - X"), None);
    assert_eq!(strip_path_component("1989 - AB"), Some("AB".to_string()));
}

// MARK: - parse_filename_stem

#[test]
fn filename_returns_stripped_stem() {
    assert_eq!(
        parse_filename_stem(Path::new("/m/Artist Name - Album Title.png")),
        vec!["Artist Name - Album Title".to_string()],
    );
}

#[test]
fn filename_strips_track_number() {
    assert_eq!(
        parse_filename_stem(Path::new("/m/01 - Back Cover.png")),
        vec!["Back Cover".to_string()],
    );
}

#[test]
fn filename_rejects_generic_names() {
    assert!(parse_filename_stem(Path::new("/m/cover.jpg")).is_empty());
    assert!(parse_filename_stem(Path::new("/m/Booklet-01.png")).is_empty());
    assert!(parse_filename_stem(Path::new("/m/disc 1.jpg")).is_empty());
}

#[test]
fn filename_keeps_non_generic_image() {
    assert_eq!(
        parse_filename_stem(Path::new("/m/Artist Name - Album.png")),
        vec!["Artist Name - Album".to_string()],
    );
}

// MARK: - catalog_numbers — single-letter-digit prefix

#[test]
fn catalogs_single_letter_digit_prefix() {
    // `Z1 12345`, `T5 67890` — single letter + single digit, then ≥4 digits.
    assert_eq!(
        cats(&["Z1 12345".to_string()]),
        vec!["Z1 12345".to_string()],
    );
    assert_eq!(
        cats(&["T5-67890".to_string()]),
        vec!["T5-67890".to_string()],
    );
}

#[test]
fn catalogs_substring_single_letter_digit() {
    // Embedded single-letter-digit catalog.
    assert_eq!(
        cats(&["Released on Z1 12345 in 2020".to_string()]),
        vec!["Z1 12345".to_string()],
    );
}

// MARK: - catalog_numbers — ZIP false positives

#[test]
fn catalogs_zip_in_po_box_rejected() {
    // Classic mailing address: state+ZIP at end of line gets stripped,
    // regardless of whether a PO Box marker is present.
    let empty: Vec<String> = Vec::new();
    assert_eq!(cats(&["P.O. Box 123, City, ZZ 12345.".to_string()]), empty,);
    assert_eq!(
        cats(&["PO Box 4567, Other City, XX 98765".to_string()]),
        empty,
    );
}

#[test]
fn catalogs_zip_tail_rejected() {
    // State abbrev + 5 digits at end of line — mailing-address tail.
    let empty: Vec<String> = Vec::new();
    assert_eq!(cats(&["Some City, NY 10001".to_string()]), empty,);
}

#[test]
fn catalogs_midline_catalog_on_mail_line_kept() {
    // A line that contains a `P.O. Box` but whose catalog-shaped
    // substring is mid-line (not at the trailing address position)
    // must keep the catalog. Only the end-of-line state+ZIP capture
    // gets dropped.
    assert_eq!(
        cats(&["P.O. Box 123 — TX 45678 is the album code".to_string()]),
        vec!["TX 45678".to_string()],
    );
}

#[test]
fn catalogs_real_catalog_beside_zip_kept() {
    // Real catalog on the same line as a ZIP-shaped string should still
    // survive. Only the ZIP-shaped candidate gets dropped.
    assert_eq!(
        cats(&["WPCR-80001 / City, NY 10001".to_string()]),
        vec!["WPCR-80001".to_string()],
    );
}

// MARK: - free_text — line-level filters

#[test]
fn free_text_rejects_track_listing() {
    assert!(kept_lines(&["1. Track Title".to_string()]).is_empty());
    assert!(kept_lines(&["  02) Track Title".to_string()]).is_empty());
    assert!(kept_lines(&[" 3.  Track Title With Spaces".to_string()]).is_empty());
}

#[test]
fn free_text_rejects_runtime_suffix() {
    assert!(kept_lines(&["Track Title (5:09)".to_string()]).is_empty());
    assert!(kept_lines(&["Something else (10:32) ".to_string()]).is_empty());
}

#[test]
fn free_text_rejects_credit_patterns() {
    assert!(kept_lines(&["Name One: bass".to_string()]).is_empty());
    assert!(kept_lines(&["Name Two: drums".to_string()]).is_empty());
    assert!(kept_lines(&["Engineered by Name Three".to_string()]).is_empty());
    assert!(kept_lines(&["Produced by Name Four".to_string()]).is_empty());
    assert!(kept_lines(&["Photography by Name Five".to_string()]).is_empty());
}

#[test]
fn free_text_rejects_legal_prose() {
    assert!(kept_lines(&["\u{00A9} 2020 Label Name. All rights reserved.".to_string()]).is_empty());
    assert!(kept_lines(&["Published by Publisher Name".to_string()]).is_empty());
    assert!(kept_lines(&["Distributed by Distributor".to_string()]).is_empty());
    assert!(kept_lines(&["P.O. Box 123, City, ZZ 12345.".to_string()]).is_empty());
    assert!(kept_lines(&["Compact Disc Digital Audio".to_string()]).is_empty());
}

#[test]
fn free_text_rejects_out_of_length_band() {
    // Too short.
    assert!(kept_lines(&["A".to_string(), "OK".to_string()]).is_empty());
    // Too long (>50 chars).
    let long = "This is a very long liner-note sentence that exceeds fifty characters handily."
        .to_string();
    assert!(kept_lines(&[long]).is_empty());
}

#[test]
fn free_text_rejects_all_digit_and_stop_phrases() {
    assert!(kept_lines(&["  1234-5678  ".to_string()]).is_empty());
    assert!(kept_lines(&["Side A".to_string()]).is_empty());
    assert!(kept_lines(&["disc 1".to_string()]).is_empty());
    assert!(kept_lines(&["CD".to_string()]).is_empty());
}

#[test]
fn free_text_keeps_real_title_with_stop_prefix() {
    // `Side A feat. Guest Artist` isn't exactly `side a` — keep it.
    assert_eq!(
        kept_lines(&["Side A feat. Guest Artist".to_string()]),
        vec!["Side A feat. Guest Artist".to_string()],
    );
}

#[test]
fn free_text_keeps_titles_with_colons_outside_credits() {
    // Title with a colon but no role keyword — not a credit pattern.
    assert_eq!(
        kept_lines(&["Album Title: The Subtitle".to_string()]),
        vec!["Album Title: The Subtitle".to_string()],
    );
}

// MARK: - normalize

#[test]
fn normalize_strips_diacritics() {
    assert_eq!(normalize("Café"), "cafe");
    assert_eq!(normalize("Fjörn"), "fjorn");
}

#[test]
fn normalize_lowercases_and_collapses_whitespace() {
    assert_eq!(normalize("  Album   Title  "), "album title");
    assert_eq!(normalize("Album\tTitle"), "album title");
}

#[test]
fn normalize_strips_leading_trailing_nonalnum() {
    assert_eq!(normalize("\"Album Title\""), "album title");
    assert_eq!(normalize("!!!Album!!!"), "album");
}

// MARK: - cluster_lines

fn artwork_line(path: &str, text: &str) -> SourcedLine {
    SourcedLine {
        source: Source::Artwork(PathBuf::from(path)),
        text: text.to_string(),
    }
}

fn path_line(text: &str) -> SourcedLine {
    SourcedLine {
        source: Source::PathComponent,
        text: text.to_string(),
    }
}

fn cue_line(text: &str) -> SourcedLine {
    SourcedLine {
        source: Source::CueField,
        text: text.to_string(),
    }
}

fn cluster_lines(lines: Vec<SourcedLine>) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    cluster_lines_incremental(&mut clusters, &lines);
    clusters
}

#[test]
fn cluster_groups_ocr_variants_together() {
    // Three spellings of one name — clean plus two garbled.
    let lines = vec![
        artwork_line("/a.jpg", "Artist Alpha"),
        artwork_line("/b.jpg", "ARTST ALPA"),  // char dropped
        artwork_line("/c.jpg", "Arist Alpha"), // char dropped
        artwork_line("/d.jpg", "Totally Different"),
    ];
    let clusters = cluster_lines(lines);
    let alpha_cluster = clusters
        .iter()
        .find(|c| c.members.len() >= 2)
        .expect("expected a multi-member cluster for OCR variants");
    assert!(
        alpha_cluster.members.len() >= 3,
        "expected 3+ variant cluster, got {:?}",
        alpha_cluster.members,
    );
}

#[test]
fn cluster_keeps_distinct_names_separate() {
    let lines = vec![
        path_line("Artist Alpha"),
        path_line("Album Title B"),
        cue_line("Name Two"),
    ];
    let clusters = cluster_lines(lines);
    assert_eq!(clusters.len(), 3);
    for c in &clusters {
        assert_eq!(c.members.len(), 1);
    }
}

/// Two incremental calls produce the same clusters as one all-at-once call —
/// what lets the service re-classify only new lines per emission.
#[test]
fn cluster_incremental_matches_whole_pool() {
    let first = vec![
        path_line("Artist Alpha"),
        artwork_line("/a.jpg", "Album Title B"),
    ];
    let second = vec![
        artwork_line("/b.jpg", "Arist Alpha"),
        artwork_line("/c.jpg", "Album Title B"),
        cue_line("Totally Different"),
    ];

    let mut incremental: Vec<Cluster> = Vec::new();
    cluster_lines_incremental(&mut incremental, &first);
    cluster_lines_incremental(&mut incremental, &second);

    let combined: Vec<SourcedLine> = first.iter().chain(second.iter()).cloned().collect();
    let all_at_once = cluster_lines(combined);

    assert_eq!(incremental.len(), all_at_once.len());
    for (a, b) in incremental.iter().zip(all_at_once.iter()) {
        assert_eq!(a.normalized_centroid, b.normalized_centroid);
        assert_eq!(a.members.len(), b.members.len());
    }
}

// MARK: - source_weight + cluster scoring

#[test]
fn cluster_score_combines_source_weights() {
    let cluster = Cluster {
        normalized_centroid: "artist alpha".to_string(),
        members: vec![
            cue_line("Artist Alpha"),               // 5
            path_line("Artist Alpha"),              // 3
            artwork_line("/a.jpg", "Artist Alpha"), // 1
        ],
    };
    assert_eq!(cluster.score(), 9);
}

// MARK: - pick_representative

#[test]
fn pick_representative_prefers_highest_source_weight() {
    let cluster = Cluster {
        normalized_centroid: "album title a".to_string(),
        members: vec![
            artwork_line("/a.jpg", "ALBM TITL A"), // garbled, weight 1
            cue_line("Album Title A"),             // clean, weight 5
        ],
    };
    assert_eq!(cluster.pick_representative(), "Album Title A");
}

#[test]
fn pick_representative_prefers_mixed_case_over_all_caps() {
    let cluster = Cluster {
        normalized_centroid: "album title".to_string(),
        members: vec![
            artwork_line("/a.jpg", "ALBUM TITLE"),
            artwork_line("/b.jpg", "Album Title"),
        ],
    };
    assert_eq!(cluster.pick_representative(), "Album Title");
}

#[test]
fn pick_representative_enforces_min_length() {
    let cluster = Cluster {
        normalized_centroid: "abc".to_string(),
        members: vec![
            artwork_line("/a.jpg", "Abc"),           // 3 chars — below 4-char floor
            artwork_line("/b.jpg", "Album Title X"), // 13 chars
        ],
    };
    assert_eq!(cluster.pick_representative(), "Album Title X");
}

#[test]
fn pick_representative_falls_back_to_first_when_all_below_floor() {
    // Every member is under the 4-char floor, so the ranking loop skips them
    // all; the fallback must return the first member, not an empty string.
    let cluster = Cluster {
        normalized_centroid: "ab".to_string(),
        members: vec![cue_line("ab"), cue_line("xy")],
    };
    assert_eq!(cluster.pick_representative(), "ab");
}

#[test]
fn apply_free_text_cutoff_gates_on_min_score_then_falls_back() {
    let make = |centroid: &str, member| Cluster {
        normalized_centroid: centroid.to_string(),
        members: vec![member],
    };

    // A CUE line scores 5 (over FREE_TEXT_MIN_SCORE); a lone artwork line
    // scores 1. Once anything clears the gate, the weak cluster is dropped.
    let gated = apply_free_text_cutoff(&[
        make("strong", cue_line("Strong Title")),
        make("weak", artwork_line("/a.jpg", "Weak Title")),
    ]);
    assert_eq!(gated, vec!["Strong Title".to_string()]);

    // When nothing clears the gate, fall back to the ranked list rather
    // than returning an empty dropdown.
    let fallback = apply_free_text_cutoff(&[make("weak", artwork_line("/a.jpg", "Weak Title"))]);
    assert_eq!(fallback, vec!["Weak Title".to_string()]);
}

/// `ARTIST NAME/ALBUM TITLE` and `ALBUM TITLE` normalize to forms whose
/// Jaro-Winkler similarity is under the 0.85 threshold, so they never share a
/// cluster and `pick_representative`'s tie-break never has to choose between
/// them. Pinned so a change to the threshold or to `normalize` surfaces here.
#[test]
fn representative_prefix_and_title_land_in_different_clusters() {
    let prefix = normalize("ARTIST NAME/ALBUM TITLE");
    let plain = normalize("ALBUM TITLE");
    let sim = jaro_winkler(&prefix, &plain);
    assert!(
        sim < JW_THRESHOLD,
        "normalized JW({prefix:?}, {plain:?}) = {sim}, expected < {JW_THRESHOLD}",
    );

    // Verify via the full clustering pipeline: two separate clusters,
    // each with exactly one member.
    let lines = vec![
        artwork_line("/a.jpg", "ARTIST NAME/ALBUM TITLE"),
        artwork_line("/b.jpg", "ALBUM TITLE"),
    ];
    let clusters = cluster_lines(lines);
    assert_eq!(
        clusters.len(),
        2,
        "expected two clusters for prefix vs plain, got {clusters:?}",
    );
}

/// Equal source weight and case rank, so length decides: the clean full form
/// beats the OCR-truncated variant of the same name.
#[test]
fn representative_longest_wins_on_ocr_truncation() {
    let cluster = Cluster {
        normalized_centroid: "album title".to_string(),
        members: vec![
            artwork_line("/a.jpg", "Album Titl"), // truncated by 1 char
            artwork_line("/b.jpg", "Album Title"),
        ],
    };
    assert_eq!(cluster.pick_representative(), "Album Title");
}
