// The scan dropping or reshaping a candidate ends the work in flight for it:
// that work read files the candidate no longer has.

/// Start extraction and an identify run on `key`, returning their tokens.
fn start_both(runtime: &CandidateRuntime, key: &str) -> [tokio_util::sync::CancellationToken; 2] {
    [CandidateWork::Extraction, CandidateWork::Identify]
        .map(|work| runtime.start_work(work, key.to_string(), |token, _| token))
}

fn invalid(candidate: &FolderCandidate) -> ImportEvent {
    ImportEvent::Scan(ScanEvent::InvalidCandidate(InvalidCandidate {
        path: candidate.path.clone(),
        name: "rel1".to_string(),
        watched_folder_path: candidate.watched_folder_path.clone(),
        display_path: "rel1".to_string(),
        grouping: candidate.grouping.clone(),
        files: Vec::new(),
        tags: None,
    }))
}

#[tokio::test]
async fn a_candidate_leaving_or_reshaped_ends_its_work() {
    for grouping in [None, Some("group-1".to_string())] {
        let mut candidate = folder_candidate("/watch/a/rel1", "/watch/a");
        candidate.grouping = grouping;
        let key = candidate.key();
        let mut reshaped = candidate.clone();
        reshaped.file_edit_revision = 1;
        let departures = [
            ("reshaped", scanned(reshaped.clone())),
            (
                "reshaped while not yet actionable",
                ImportEvent::Scan(ScanEvent::CandidateDiscovered {
                    candidate: reshaped,
                }),
            ),
            (
                "rebound",
                ImportEvent::Scan(ScanEvent::CandidateBindingChanged {
                    candidate: candidate.clone(),
                }),
            ),
            ("invalid", invalid(&candidate)),
            (
                "removed",
                ImportEvent::Scan(ScanEvent::CandidateRemoved {
                    candidate_key: key.clone(),
                }),
            ),
        ];
        for (name, departure) in departures {
            let runtime = CandidateRuntime::for_test().await;
            runtime.record_event(&scanned(candidate.clone()));
            let tokens = start_both(&runtime, &key);
            let other = start_both(&runtime, "/watch/a/other");

            runtime.record_event(&departure);

            assert!(
                tokens.iter().all(|token| token.is_cancelled()),
                "{key} {name}: its work goes on"
            );
            assert!(
                other.iter().all(|token| !token.is_cancelled()),
                "{key} {name}: another key's work ended"
            );
        }
    }
}
