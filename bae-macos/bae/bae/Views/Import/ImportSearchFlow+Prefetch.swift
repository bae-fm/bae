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
            // Unknown imports never carry a source release — clear any
            // prior detail and seed so the confirmation page falls back to its
            // detail-less rendering (no remote cover picker, no
            // library-status banner, no track-count mismatch, no
            // Exact/Metadata choice).
            candidate.releaseDetailBridge = nil
            candidate.releaseSeed = nil
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

    /// The pressing the user picked to prefetch: the search `result`, the
    /// `identityChoice` made at row-time (carried through to the confirmation
    /// page so commit applies it), and the local track count to reconcile the
    /// fetched detail against.
    struct PrefetchSelection {
        let result: BridgeMetadataResult
        let identityChoice: BridgeIdentityChoice
        let localTrackCount: UInt32?
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
            // The choice was made at row-time. Carry it through
            // prefetch into the confirmation page so commit can apply it.
            candidate.identityChoice = selection.identityChoice
        }

        let releaseId = selection.result.releaseId
        let bridgeSource = selection.result.source
        let task = Task { @MainActor in
            do {
                let prefetch = try await library.prefetchRelease(
                    releaseId,
                    bridgeSource,
                    selection.localTrackCount
                )
                let preview = shapeUserEditForChoice(
                    seed: prefetch.seed,
                    choice: selection.identityChoice
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    // The display detail: cover options, track-count mismatch,
                    // library status.
                    candidate.releaseDetailBridge = prefetch.detail
                    // The editor's seed. Kept so flipping the Exact /
                    // Metadata-only choice in the pane re-shapes it without a
                    // re-fetch.
                    candidate.releaseSeed = prefetch.seed
                    // Manual prefetch: the user just picked a different
                    // release, so any prior pick was for a now-stale cover
                    // set — replace it with the new release's default.
                    candidate.selectedCover = prefetch.detail.defaultCover
                    // The seed arrives pre-shaped from bae-core, which projects
                    // it from the release the way the commit worker maps it;
                    // `shapeUserEditForChoice` masks the pressing fields per the
                    // identity claim. `rawReleaseEditFromUserEdit` projects that
                    // wire edit into the raw form the editor binds.
                    candidate.editValues = rawReleaseEditFromUserEdit(
                        edit: preview,
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
