import BaeKit
import os.log

private let logger = Logger.bae("ImportSearchFlow")

extension ImportSearchFlow {
    // MARK: - Add as Unknown

    /// Project the candidate's audio files into a `ReleaseUserEdit`
    /// shape via the bridge's file-tag preview, seed the editor with
    /// the result, mark the choice as Unknown, and transition to the
    /// confirming mode. Errors fall back to the identifying state with
    /// a banner so the user can retry or pick a search match instead.
    @MainActor
    static func addAsUnknown(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        folderPath: String
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mode = .loadingDetail
            candidate.error = nil
            candidate.identityChoice = .unknown
            // Unknown imports never carry a source release — clear any prior
            // detail and claim so the confirmation page falls back to its
            // detail-less rendering (no remote cover picker, no library-status
            // banner, no track-count mismatch, no claim line: there is no
            // release to claim and none the metadata came from).
            candidate.releaseDetailBridge = nil
            candidate.claim = nil
            candidate.pickedReleaseId = nil
            // No source cover exists for Unknown; leave the local
            // artwork picker as the only cover affordance.
            candidate.selectedCover = nil
        }

        let task = Task { @MainActor in
            do {
                let preview = try await importer.previewFileTagsForFolder(
                    folderPath
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.editValues = rawReleaseEditFromUserEdit(
                        edit: preview,
                        trackIdPrefix: "unknown-track"
                    )
                    candidate.mode = .confirming
                    candidate.prefetchTask = nil
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Add as Unknown cancelled for key: \(key)"
                )
            }
            catch {
                logger.error(
                    "Add as Unknown failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.mode = .identifying
                    candidate.identityChoice = nil
                    candidate.error =
                        "Couldn't read file tags: \(error.displayLine)"
                    candidate.prefetchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.prefetchTask = CancelOnDeinit(task)
        }
    }

    // MARK: - Prefetch and confirm

    /// The pressing the user picked to prefetch. What the pick claims is not
    /// here, and neither is how it lines up with the folder's audio — bae-core
    /// derives both from the candidate and returns them with the prefetch.
    struct PrefetchSelection {
        let result: BridgeMetadataResult
    }

    @MainActor
    static func prefetchAndConfirm(
        library: Library,
        importStore: ImportStore,
        key: String,
        selection: PrefetchSelection
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.mode = .loadingDetail
            candidate.error = nil
            // Hold the clicked release so its row stays selected while the
            // prefetch runs; the claim it implies arrives with the prefetch.
            candidate.pickedReleaseId = selection.result.releaseId
        }

        let releaseId = selection.result.releaseId
        let bridgeSource = selection.result.source
        let task = Task { @MainActor in
            do {
                let prefetch = try await library.prefetchRelease(
                    key,
                    releaseId,
                    bridgeSource
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    // The display detail: cover options, library status.
                    candidate.releaseDetailBridge = prefetch.detail
                    // What the pick claims, derived in bae-core from the
                    // evidence that identified this candidate. The header
                    // states it; the command carries it.
                    candidate.claim = prefetch.claim
                    candidate.identityChoice = prefetch.claim.choice
                    // Manual prefetch: the user just picked a different
                    // release, so any prior pick was for a now-stale cover
                    // set — replace it with the new release's default.
                    candidate.selectedCover = prefetch.detail.defaultCover
                    // The seed arrives from bae-core projected the way the
                    // commit worker maps the release, and already masked for
                    // the claim (an album-level claim blanks the pressing
                    // block). `rawReleaseEditFromUserEdit` projects that wire
                    // edit into the raw form the editor binds.
                    candidate.editValues = rawReleaseEditFromUserEdit(
                        edit: prefetch.seed,
                        trackIdPrefix: "import-track"
                    )
                    candidate.mode = .confirming
                    candidate.prefetchTask = nil
                }
            }
            catch is CancellationError {
                logger.debug(
                    "Prefetch cancelled for key: \(key)"
                )
            }
            catch {
                logger.error(
                    "Prefetch failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    candidate.mode = .identifying
                    candidate.error =
                        "Failed to load release details: \(error.displayLine)"
                    // The pick never resolved, so nothing is claimed and no row
                    // is selected. All three drop together: leaving the
                    // previous pick's claim behind would state a claim for a
                    // release the pane is no longer showing.
                    candidate.pickedReleaseId = nil
                    candidate.claim = nil
                    candidate.identityChoice = nil
                    candidate.prefetchTask = nil
                }
            }
        }

        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.prefetchTask = CancelOnDeinit(task)
        }
    }

    // MARK: - Import status helpers

    @MainActor
    static func isImporting(_ candidate: Candidate) -> Bool {
        guard let status = candidate.importStatus else {
            return false
        }
        switch status {
        case .importing, .complete: return true
        default: return false
        }
    }

}
