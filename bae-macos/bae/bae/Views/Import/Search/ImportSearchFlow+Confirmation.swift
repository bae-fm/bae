import BaeKit
import SwiftUI

extension ImportSearchFlow {
    // MARK: - Import-as choice (in the pane)

    /// Flip the open pane's Exact / Metadata-only choice and re-seed the
    /// editor from the stored seed. Exact keeps the picked release's pressing
    /// fields; Metadata-only blanks them. Re-shaping is bae-core's job —
    /// `shape_user_edit_for_choice` masks the pressing fields per the choice —
    /// so this re-runs it rather than mutating fields in Swift.
    ///
    /// `seed` and `ref` come from the call site (the toggle only renders for
    /// a source-backed pick, so both are in hand there) — no in-closure lookup
    /// or guard.
    @MainActor
    static func changeChoice(
        importStore: ImportStore,
        key: String,
        seed: BridgeReleaseUserEdit,
        ref: (releaseId: String, source: BridgeMetadataSource),
        wantExact: Bool
    ) {
        let choice = BridgeIdentityChoice.make(
            exact: wantExact,
            releaseId: ref.releaseId,
            source: ref.source
        )
        importStore.mutateCandidate(forKey: key) { candidate in
            candidate.identityChoice = choice
            let preview = shapeUserEditForChoice(
                seed: seed,
                choice: choice
            )
            candidate.editValues = rawReleaseEditFromUserEdit(
                edit: preview,
                trackIdPrefix: "import-track"
            )
        }
    }

    // MARK: - Shared confirmation view builder

    /// Two-way binding into the candidate's `editValues` field. Edits
    /// from the embedded edit-metadata form route through here into the
    /// import store; commit reads the live value to build the import
    /// command's `user_edit` overlay.
    @MainActor
    static func makeEditValuesBinding(
        importStore: ImportStore,
        key: String,
        candidate: Candidate
    ) -> Binding<BridgeRawReleaseEdit> {
        // The candidate's editValues was seeded by prefetchAndConfirm
        // before transitioning to .confirming, so the optional is
        // populated by the time this binding is read. Force-unwrap on
        // get keeps the binding non-optional for the form.
        Binding(
            get: {
                guard let values = candidate.editValues else {
                    preconditionFailure(
                        "editValues must be seeded before the confirm binding is read"
                    )
                }
                return values
            },
            set: { newValue in
                importStore.mutateCandidate(forKey: key) {
                    $0.editValues = newValue
                }
            },
        )
    }

    /// The candidate, services, and source-detail-derived display inputs a
    /// confirmation view renders. The detail fields (track-count mismatch,
    /// library status, remote cover art) are discrete rather than a whole
    /// `BridgeReleaseDetail`, so Unknown imports can supply their
    /// file-tag-derived equivalents (no source release id, no remote cover art,
    /// no track-count source to mismatch against).
    struct ConfirmationInputs {
        let importStore: ImportStore
        let key: String
        let uiStore: UiStore
        let trackCountMismatch: Bool
        let expectedTrackCount: UInt32
        let libraryStatus: BridgeLibraryStatus?
        let remoteCoverArts: [BridgeRemoteCover]
        let hasCoverOptions: Bool
        let storageManaged: Binding<Bool>
        let storagePinned: Binding<Bool>
        let localArtwork: [BridgeCandidateFile]
    }

    /// The confirmation view's action callbacks: commit the import, and open the
    /// just-imported release in the library.
    struct ConfirmationCallbacks {
        let onConfirmImport: () -> Void
        let onViewInLibrary: (String) -> Void
    }

    /// Build the confirmation view for a candidate.
    @MainActor
    @ViewBuilder
    static func buildConfirmationView(
        inputs: ConfirmationInputs,
        callbacks: ConfirmationCallbacks,
        @ViewBuilder coverContent: @escaping () -> some View
    ) -> some View {
        let importStore = inputs.importStore
        let key = inputs.key
        let uiStore = inputs.uiStore
        let candidate = importStore.candidate(forKey: key)
        let selectedCover = candidate?.selectedCover
        let importing = candidate.map(isImporting) ?? false

        if let candidate {
            ImportConfirmationView(
                values: makeEditValuesBinding(
                    importStore: importStore,
                    key: key,
                    candidate: candidate
                ),
                storageManaged: inputs.storageManaged,
                storagePinned: inputs.storagePinned,
                trackCountMismatch: inputs.trackCountMismatch,
                expectedTrackCount: inputs.expectedTrackCount,
                libraryStatus: inputs.libraryStatus,
                candidateKey: key,
                importStatus: candidate.importStatus,
                error: candidate.error,
                hasCoverOptions: inputs.hasCoverOptions,
                importing: importing,
                exactness: exactnessChoice(
                    for: candidate,
                    importStore: importStore,
                    key: key
                ),
                onConfirmImport: callbacks.onConfirmImport,
                onViewInLibrary: callbacks.onViewInLibrary,
                onEditCover: {
                    uiStore.presentModal {
                        CoverPickerView(
                            remoteCoverArts: inputs.remoteCoverArts,
                            localArtwork: inputs.localArtwork,
                            selectedCover: selectedCover,
                            onSelect: { selection in
                                importStore.mutateCandidate(forKey: key) {
                                    $0.selectedCover = selection
                                }
                                uiStore.dismissModal()
                            },
                            onDone: { uiStore.dismissModal() },
                        )
                        .frame(width: 600, height: 500)
                    }
                },
                coverContent: coverContent,
            )
        }
    }

    /// The Exact / Metadata-only toggle, or `nil` when it doesn't apply. The
    /// toggle renders only for a source-backed pick (one with a stored seed and
    /// a release ref); Unknown imports have neither and get no toggle.
    /// Unwrapping the seed/ref here means `changeChoice` needs no in-closure
    /// guard.
    @MainActor
    private static func exactnessChoice(
        for candidate: Candidate,
        importStore: ImportStore,
        key: String
    ) -> ImportExactnessChoice? {
        guard let seed = candidate.releaseSeed,
            let choice = candidate.identityChoice,
            let ref = choice.releaseRef
        else {
            return nil
        }
        return ImportExactnessChoice(
            isMetadataOnly: choice.isApproximate,
            onSelect: { wantExact in
                changeChoice(
                    importStore: importStore,
                    key: key,
                    seed: seed,
                    ref: ref,
                    wantExact: wantExact
                )
            }
        )
    }
}
