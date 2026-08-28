import BaeKit
import os.log

private let logger = Logger.bae("ImportSearchFlow")

extension ImportSearchFlow {
    // MARK: - Deciding the identity

    /// Decide the candidate's identity — a pressing, or the folder's own tags.
    ///
    /// Nothing is seeded from the answer: core archives the release's
    /// documents, stores the pick, and the per-candidate read delivers the
    /// pane's next value. All this holds is the flag the clicked control reads
    /// as pending, and the line a failure leaves on the banner.
    @MainActor
    static func selectMetadataSeed(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        seed: BridgeMetadataSeed,
        onConfirmed: (@Sendable () -> Void)? = nil
    ) {
        guard
            let session = importStore.beginMetadataSeedSelection(
                key: key,
                seed: seed,
                onConfirmed: onConfirmed
            )
        else {
            logger.debug("Identity pick ignored for missing key: \(key)")
            return
        }

        let task = Task { @MainActor [weak session] in
            do {
                try await importer.selectCandidateMetadataSeed(key, seed)
                guard let session else { return }
                importStore.metadataSeedCommandSucceeded(
                    key: key,
                    session: session
                )
            }
            catch is CancellationError {
                logger.debug("Identity pick cancelled for key: \(key)")
                guard let session else { return }
                importStore.metadataSeedSelectionFailed(
                    key: key,
                    session: session,
                    error: nil
                )
            }
            catch {
                logger.error(
                    "Identity pick failed: \(error.localizedDescription)"
                )
                guard let session else { return }
                let line = error.displayLine.map {
                    switch seed {
                    case .externalRelease:
                        String(
                            localized:
                                "Failed to load release details: \($0)"
                        )
                    case .fileTags:
                        String(localized: "Couldn't read file tags: \($0)")
                    case .manual:
                        String(localized: "Couldn't save that change: \($0)")
                    }
                }
                importStore.metadataSeedSelectionFailed(
                    key: key,
                    session: session,
                    error: line
                )
            }
        }
        session.install(task)
    }

    /// Pick one search-sheet pressing and dismiss only after its command and
    /// exact candidate-detail delivery have both succeeded.
    @MainActor
    static func chooseReleaseFromSearchSheet(
        _ result: BridgeMetadataResult,
        importer: Importer,
        importStore: ImportStore,
        key: String,
        onConfirmed: @escaping @Sendable () -> Void
    ) {
        selectMetadataSeed(
            importer: importer,
            importStore: importStore,
            key: key,
            seed: .externalRelease(
                source: result.source,
                releaseId: result.releaseId
            ),
            onConfirmed: onConfirmed
        )
    }

    // MARK: - Import status helpers

    /// Whether the candidate's import has been committed to — running, or
    /// already done. Either way the search is spent: what it would change was
    /// settled when the import started.
    @MainActor
    static func isImporting(_ candidate: Candidate) -> Bool {
        switch candidate.row?.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
    }

}
