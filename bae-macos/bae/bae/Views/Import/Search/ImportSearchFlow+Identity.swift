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
            candidate.pickInFlight = pick
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
            importStore.mutateCandidate(forKey: key) { candidate in
                if candidate.pickInFlight == pick {
                    candidate.pickInFlight = nil
                }
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
