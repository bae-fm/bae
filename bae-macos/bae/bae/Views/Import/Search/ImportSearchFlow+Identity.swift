import BaeKit
import os.log

private let logger = Logger.bae("ImportSearchFlow")

extension ImportSearchFlow {
    // MARK: - Applying metadata

    /// Apply one metadata source. The source browser stays where it is until
    /// core holds the draft this read produces; the pick then puts the draft
    /// back in the metadata slot, whatever is being looked at by then.
    @MainActor
    static func applyMetadata(
        importer: Importer,
        importStore: ImportStore,
        endEditing: @escaping @MainActor () async -> Void,
        key: String,
        application: MetadataApplication
    ) {
        guard
            let session = importStore.beginMetadataApplication(
                key: key,
                application: application
            )
        else {
            logger.debug("Metadata application ignored for missing key: \(key)")
            return
        }

        let task = Task { @MainActor [weak session] in
            await endEditing()
            let result: Result<BridgePaneOutcome, Error>
            do {
                result = .success(
                    try await pick(application, key: key, importer: importer)
                )
            }
            catch { result = .failure(error) }
            guard let session else { return }
            settle(
                result,
                application: application,
                key: key,
                session: session,
                importStore: importStore
            )
        }
        session.install(task)
    }

    /// Run the pick core-side.
    @MainActor
    private static func pick(
        _ application: MetadataApplication,
        key: String,
        importer: Importer
    ) async throws -> BridgePaneOutcome {
        switch application {
        case .pick(let link):
            try await importer.applyCandidateExternalMetadata(key, link: link)
        case .fileTags:
            try await importer.applyCandidateFileMetadata(key)
        }
    }

    /// End the pick as it came out. A pane command's failure is stated on the
    /// pane from what core stored; a catalog release that failed to load says
    /// so on its own row; any other failure is told to the person.
    @MainActor
    private static func settle(
        _ result: Result<BridgePaneOutcome, Error>,
        application: MetadataApplication,
        key: String,
        session: CandidateMetadataApplicationSession,
        importStore: ImportStore
    ) {
        switch result {
        case .success(.done):
            importStore.metadataApplicationSucceeded(key: key, session: session)
        case .success(.failed):
            importStore.metadataApplicationFailed(
                key: key,
                session: session,
                error: nil
            )
        case .failure(is CancellationError):
            logger.debug("Metadata application cancelled for key: \(key)")
            importStore.metadataApplicationFailed(
                key: key,
                session: session,
                error: nil
            )
        case .failure(let error):
            logger.error(
                "Metadata application failed: \(error.localizedDescription)"
            )
            if case .pick = application {
                importStore.metadataApplicationFailed(
                    key: key,
                    session: session,
                    error: metadataApplicationError(error)
                )
                return
            }
            importStore.metadataApplicationFailed(
                key: key,
                session: session,
                error: nil
            )
            importStore.reportFailure(error)
        }
    }

    /// A catalog release that failed to load, as its row says it.
    private static func metadataApplicationError(_ error: Error)
        -> DisplayError?
    {
        guard let displayed = DisplayError(error) else { return nil }
        let detail: String?
        if case BridgeError.Diagnostic(let category, let diagnostic) = error {
            switch category {
            case .importData, .database, .internal, .config:
                detail = diagnostic
            default:
                detail = nil
            }
            if category == .metadataTrackCount {
                return DisplayError(line: displayed.line)
            }
        }
        else {
            detail = displayed.detail
        }
        return DisplayError(
            line: String(
                localized: "Failed to load release details: \(displayed.line)"
            ),
            detail: detail
        )
    }

    // MARK: - Import status helpers

    /// Whether the candidate's import has been committed to — running, or
    /// already done. Either way the search is spent: what it would change was
    /// settled when the import started.
    @MainActor
    static func isImporting(_ candidate: Candidate) -> Bool {
        switch candidate.importStatus {
        case .importing, .complete: return true
        case .error, nil: return false
        }
    }

}
