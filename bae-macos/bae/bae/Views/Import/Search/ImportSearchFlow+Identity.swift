import BaeKit
import os.log

private let logger = Logger.bae("ImportSearchFlow")

extension ImportSearchFlow {
    // MARK: - Deciding and re-applying the identity

    /// Decide the candidate's identity — a pressing, or its own tags — and
    /// seed the pane from what the decision stands for. Core persists the
    /// choice and returns the same payload a later selection's query serves,
    /// so a fresh launch renders exactly what this click rendered.
    @MainActor
    static func decideIdentity(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        pick: BridgeIdentityPick
    ) {
        applyIdentity(importStore: importStore, key: key, pick: pick) {
            try await importer.pickCandidateIdentity(key, pick)
        }
    }

    /// Re-apply the identity already decided for the candidate — a selection
    /// finding a stored decision, or a shape change re-deriving under one.
    /// Nothing here re-persists: the decision is already stored, which is
    /// where it came from. A `nil` answer (the decision vanished under a
    /// racing edit) backs the pane out to undecided.
    @MainActor
    static func refreshDecidedIdentity(
        importer: Importer,
        importStore: ImportStore,
        key: String,
        pick: BridgeIdentityPick
    ) {
        applyIdentity(importStore: importStore, key: key, pick: pick) {
            try await importer.candidateDecidedIdentity(key)
        }
    }

    /// The shared shape of the command and the query: show the decision
    /// immediately — the clicked row's spinner, or the Unknown side of the
    /// control — run the operation, and seed the pane from its answer.
    @MainActor
    private static func applyIdentity(
        importStore: ImportStore,
        key: String,
        pick: BridgeIdentityPick,
        operation: @escaping () async throws -> BridgeDecidedIdentity?
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.error = nil
            switch pick {
            case .release(let source, let releaseId, let claim):
                candidate.identity = .release
                // Hold the decision so its row carries the spinner while the
                // read runs; the line it reads as arrives with the answer.
                candidate.pick = CandidatePick(
                    releaseId: releaseId,
                    source: source,
                    claim: claim
                )
            case .unknown:
                candidate.identity = .unknown
                candidate.identityChoice = .unknown
                // Unknown imports never carry a source release — clear the
                // detail and the claim so the pane falls back to its
                // detail-less rendering. `pick` stays: it is what switching
                // back re-picks.
                candidate.releaseDetailBridge = nil
                candidate.claim = nil
                candidate.coverPick = nil
            }
        }

        let task = Task { @MainActor in
            do {
                guard let answer = try await operation() else {
                    // The stored decision vanished between the row and the
                    // read — an edit raced it. Back out to undecided; the
                    // row's next tick re-asks.
                    logger.debug("no decided identity to re-apply for \(key)")
                    importStore.mutateCandidate(forKey: key) { candidate in
                        candidate.identity = .release
                        candidate.pick = nil
                        candidate.identityChoice = nil
                        candidate.prefetchTask = nil
                    }
                    return
                }
                seed(answer, importStore: importStore, key: key)
            }
            catch is CancellationError {
                logger.debug("Identity read cancelled for key: \(key)")
            }
            catch {
                logger.error(
                    "Identity read failed: \(error.localizedDescription)"
                )
                importStore.mutateCandidate(forKey: key) { candidate in
                    switch pick {
                    case .release:
                        candidate.error =
                            "Failed to load release details: \(error.displayLine)"
                    case .unknown:
                        candidate.error =
                            "Couldn't read file tags: \(error.displayLine)"
                        candidate.identity = .release
                    }
                    // The decision never resolved on screen, so nothing is
                    // claimed and no row renders selected. All three drop
                    // together: leaving the previous pick's claim behind
                    // would state a claim for a release the pane is no
                    // longer showing.
                    candidate.pick = nil
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

    /// Land the answer on the candidate — the one place either identity's
    /// payload becomes pane state.
    @MainActor
    private static func seed(
        _ answer: BridgeDecidedIdentity,
        importStore: ImportStore,
        key: String
    ) {
        importStore.mutateCandidate(forKey: key) { candidate in
            switch answer {
            case .release(let source, let releaseId, let prefetch):
                candidate.identity = .release
                candidate.pick = CandidatePick(
                    releaseId: releaseId,
                    source: source,
                    claim: prefetch.claim.level
                )
                // The display detail: cover options, library status.
                candidate.releaseDetailBridge = prefetch.detail
                // What the decision claims, as bae-core reads it back off the
                // stored pick. The header states it; the commit carries it.
                candidate.claim = prefetch.claim
                candidate.identityChoice = prefetch.claim.choice
                // No pick yet: the pane shows the release's own first cover
                // option through `coverFace`, and a commit with nothing picked
                // lands that same option from bae-core.
                candidate.coverPick = nil
                // The seed arrives from bae-core projected the way the
                // commit worker maps the release, and already masked for the
                // claim (an album-level claim blanks the pressing block).
                candidate.editValues = albumEdit(from: prefetch.seed)
                // What the claim is a claim about: editing the fields away
                // from these lowers it, which core decides.
                candidate.exactPressing = prefetch.exactPressing
                // The mapping this decision produces: every source unit the
                // folder offers with the track committing makes of it.
                candidate.mapping = prefetch.mapping
            case .unknown(let seed, let mapping):
                candidate.identity = .unknown
                candidate.identityChoice = .unknown
                candidate.releaseDetailBridge = nil
                candidate.claim = nil
                candidate.coverPick = nil
                candidate.editValues = albumEdit(from: seed)
                candidate.exactPressing = nil
                candidate.mapping = mapping
            }
            candidate.prefetchTask = nil
        }
    }

    // MARK: - Picked resume

    /// The identity a selected candidate's row already carries — the settled
    /// single match, or the choice made earlier — applied without a click.
    /// `nil` when there is nothing to apply: something is already settled or
    /// in flight for this folder, a prefetch failed and is waiting on the
    /// user, or the row is past deciding.
    ///
    /// The placement gate carries weight the settled check can't: Done and
    /// Skipped rows keep their pick too, and a candidate rebuilt at launch
    /// starts back with nothing settled, so without it an already-imported
    /// row would re-open a commit-able pane.
    static func pickedResume(
        candidate: Candidate,
        row: BridgeTriageRow?
    ) -> BridgeIdentityPick? {
        guard candidate.identityChoice == nil,
            candidate.pick == nil,
            candidate.error == nil,
            let row
        else {
            return nil
        }
        switch row.placement {
        case .ready, .needsYou:
            return row.picked
        case .importing, .done, .skipped:
            return nil
        }
    }

    /// The album fields the editor holds, projected from a seed. The tracklist
    /// is the mapping table's — the row that produces a track is the row that
    /// edits it — so the editor carries none of its own, and the commit reads
    /// the table's rows alongside these fields.
    @MainActor
    private static func albumEdit(
        from seed: BridgeReleaseUserEdit
    ) -> BridgeRawReleaseEdit {
        var edit = rawReleaseEditFromUserEdit(
            edit: seed,
            trackIdPrefix: "import-track"
        )
        edit.tracks = []
        return edit
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
