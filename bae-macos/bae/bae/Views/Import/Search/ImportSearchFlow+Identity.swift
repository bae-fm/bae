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
    static func decideIdentity(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        pick: BridgeIdentityPick
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.error = nil
            candidate.pickInFlight = true
        }
        Task { @MainActor in
            do {
                try await importer.pickCandidateIdentity(key, pick)
            }
            catch is CancellationError {
                logger.debug("Identity pick cancelled for key: \(key)")
            }
            catch {
                logger.error(
                    "Identity pick failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    // A nil line is core reporting a cancellation: nothing to
                    // put in front of the user, and the pane still shows
                    // whatever was stored before the click.
                    candidate.error = error.displayLine.map {
                        switch pick {
                        case .release:
                            String(
                                localized:
                                    "Failed to load release details: \($0)"
                            )
                        case .unknown:
                            String(localized: "Couldn't read file tags: \($0)")
                        }
                    }
                }
            }
            importStore.mutateCandidate(forKey: key) { $0.pickInFlight = false }
        }
    }

    // MARK: - Import status helpers

    @MainActor
    static func isImporting(_ candidate: Candidate) -> Bool {
        guard let status = candidate.importStatus else {
            return false
        }
        switch status {
        case .importing, .complete, .cloudUploadQueued: return true
        default: return false
        }
    }

}
