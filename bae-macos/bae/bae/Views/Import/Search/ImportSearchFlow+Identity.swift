import BaeKit
import os.log

private let logger = Logger.bae("ImportSearchFlow")

extension ImportSearchFlow {
    // MARK: - Applying metadata

    /// Apply one metadata source. The source browser remains visible until the
    /// command and the exact authoritative detail have both arrived.
    @MainActor
    static func applyMetadata(
        importer: Importer,
        importStore: ImportStore,
        endEditing: () -> Void,
        key: String,
        provenance: BridgeMetadataProvenance,
        onConfirmed: (() -> Void)? = nil
    ) {
        endEditing()
        guard
            let session = importStore.beginMetadataApplication(
                key: key,
                provenance: provenance,
                onConfirmed: onConfirmed
            )
        else {
            logger.debug("Metadata application ignored for missing key: \(key)")
            return
        }

        let task = Task { @MainActor [weak session] in
            do {
                let revision =
                    switch provenance {
                    case .externalRelease(let source, let releaseId):
                        try await importer.applyCandidateExternalMetadata(
                            key,
                            source: source,
                            releaseId: releaseId
                        )
                    case .fileTags:
                        try await importer.applyCandidateFileTags(key)
                    }
                guard let session else { return }
                importStore.metadataApplicationCommandSucceeded(
                    key: key,
                    session: session,
                    revision: revision
                )
            }
            catch is CancellationError {
                logger.debug("Metadata application cancelled for key: \(key)")
                guard let session else { return }
                importStore.metadataApplicationFailed(
                    key: key,
                    session: session,
                    error: nil
                )
            }
            catch {
                logger.error(
                    "Metadata application failed: \(error.localizedDescription)"
                )
                guard let session else { return }
                importStore.metadataApplicationFailed(
                    key: key,
                    session: session,
                    error: metadataApplicationError(
                        error,
                        provenance: provenance
                    )
                )
            }
        }
        session.install(task)
    }

    private static func metadataApplicationError(
        _ error: Error,
        provenance: BridgeMetadataProvenance
    ) -> String? {
        error.displayLine.map {
            switch provenance {
            case .externalRelease:
                String(localized: "Failed to load release details: \($0)")
            case .fileTags:
                String(localized: "Couldn't read file tags: \($0)")
            }
        }
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
