use super::*;
use std::path::PathBuf;

// MARK: - Pool::classify end-to-end (one-shot on a fresh pool)

fn cue_line(text: &str) -> SourcedLine {
    SourcedLine::new(Source::CueField { file_id: "Album.cue".to_string() }, text.to_string())
}

fn path_line(text: &str) -> SourcedLine {
    SourcedLine::new(Source::PathComponent, text.to_string())
}

fn artwork_line(path: &str, text: &str) -> SourcedLine {
    SourcedLine::new(Source::Artwork { path: PathBuf::from(path) }, text.to_string())
}

/// Push lines + brackets into a fresh pool and classify once, catalogs
/// projected back to bare strings.
fn classify_pool(lines: Vec<SourcedLine>, brackets: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut pool = Pool::default();
    for line in lines {
        pool.push(line);
    }
    for b in brackets {
        pool.push_bracket((*b).to_string());
    }
    let classification = pool.classify();
    (classification.catalogs, classification.free_text)
}

#[test]
fn classify_promotes_high_score_clusters() {
    // Three sources agree on "Artist Alpha"; the reject rules drop the
    // one-off OCR credit line entirely.
    let lines = vec![
        cue_line("Artist Alpha"),
        path_line("Artist Alpha"),
        artwork_line("/a.jpg", "Artist Alpha"),
        artwork_line("/b.jpg", "Engineered by Name One"),
    ];
    let (_catalogs, free_text) = classify_pool(lines, &[]);
    assert!(
        free_text.iter().any(|s| s == "Artist Alpha"),
        "expected Artist Alpha to dominate, got {free_text:?}",
    );
    assert!(
        !free_text.iter().any(|s| s.contains("Engineered by")),
        "credit line should have been filtered, got {free_text:?}",
    );
}

#[test]
fn classify_includes_bracket_catalogs_in_catalog_pool() {
    let (catalogs, _) = classify_pool(vec![cue_line("Artist Alpha")], &["XX34b"]);
    assert_eq!(catalogs, vec!["XX34b".to_string()]);
}

#[test]
fn classify_falls_back_when_no_cluster_clears_threshold() {
    // One artwork image, so every cluster is a score-1 singleton. The pool
    // must fall back to the top N rather than come back empty.
    let lines = vec![
        artwork_line("/a.jpg", "Artist Alpha"),
        artwork_line("/a.jpg", "Album Title B"),
        artwork_line("/a.jpg", "Label Name"),
    ];
    let (_, free_text) = classify_pool(lines, &[]);
    assert!(!free_text.is_empty(), "expected fallback top-N pool");
}
