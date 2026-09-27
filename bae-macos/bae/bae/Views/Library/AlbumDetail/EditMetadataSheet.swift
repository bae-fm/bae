import BaeKit
import SwiftUI

/// Library presentation shell around the shared persisted-release editor.
struct EditMetadataSheet: View {
    let onCancel: @MainActor @Sendable () -> Void
    let onSaved: @MainActor @Sendable () -> Void

    @State
    private var session: ReleaseMetadataEditSession
    @State
    private var showingCoverPicker = false
    @Environment(ReleaseEditor.self)
    private var releaseEditor

    init(
        releaseId: String,
        seed: BridgeReleaseEditSeed,
        onSave:
            @escaping @Sendable (
                BridgeReleaseUserEdit
            ) async throws -> Void,
        onReset: @escaping @Sendable () async throws -> BridgeRawReleaseEdit,
        onSaved: @escaping @MainActor @Sendable () -> Void,
        onCancel: @escaping @MainActor @Sendable () -> Void
    ) {
        self.onCancel = onCancel
        self.onSaved = onSaved
        _session = State(
            initialValue: ReleaseMetadataEditSession(
                releaseId: releaseId,
                seed: seed,
                save: { _, edit in try await onSave(edit) },
                reset: { _ in try await onReset() }
            )
        )
    }

    var body: some View {
        GeometryReader { geometry in
            let size = Self.modalSize(in: geometry.size)
            VStack(spacing: 0) {
                header
                Divider()
                ScrollView {
                    ReleaseMetadataEditorContent(
                        session: session,
                        onEditCover: { showingCoverPicker = true }
                    )
                    .padding(ThemeSpace.section)
                }
                footer
            }
            .frame(width: size.width, height: size.height)
            .background(Theme.background)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .sheet(isPresented: $showingCoverPicker) {
                CoverSheetView(
                    releaseId: session.releaseId,
                    fetchRemoteCovers: {
                        try await releaseEditor.fetchRemoteCovers(
                            .release(releaseId: session.releaseId)
                        )
                    },
                    onSelect: { selection in
                        try await releaseEditor.changeCover(
                            session.releaseId,
                            selection
                        )
                        let seed = try await releaseEditor.seedReleaseEdit(
                            session.releaseId
                        )
                        session.updateCover(seed.cover)
                    },
                    onDone: { showingCoverPicker = false }
                )
                .frame(
                    width: min(1_000, size.width),
                    height: min(740, size.height)
                )
            }
        }
        .onDisappear { session.cancelTasks() }
    }

    /// How much narrower and shorter than its host the sheet sits.
    private static let hostMargin: CGFloat = 80

    static func modalSize(in host: CGSize) -> CGSize {
        CGSize(
            width: min(
                host.width,
                min(1_200, max(760, host.width - hostMargin))
            ),
            height: min(
                host.height,
                min(860, max(600, host.height - hostMargin))
            )
        )
    }

    var resetButtonIsVisible: Bool {
        session.canResetToSource
    }

    private var header: some View {
        HStack {
            Text("Edit Metadata").themeText(.heading)
            Spacer()
            Button("Cancel") { onCancel() }
                .keyboardShortcut(.cancelAction)
                .disabled(session.isBusy)
        }
        .padding()
    }

    private var footer: some View {
        VStack(spacing: ThemeSpace.related) {
            if let message = session.validationMessage
                ?? session.failureMessage
            {
                HStack(spacing: ThemeSpace.inline) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message)
                }
                .themeText(.body)
                .foregroundStyle(Theme.danger)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Reset to Source") { session.resetToSource() }
                    .disabled(session.isBusy)
                    .opacity(resetButtonIsVisible ? 1 : 0)
                    .allowsHitTesting(resetButtonIsVisible)
                Spacer()
                if session.isBusy {
                    ProgressView().controlSize(.small)
                    Text(session.isSaving ? "Saving..." : "Resetting...")
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                }
                else {
                    Button("Save") {
                        session.save(onSuccess: onSaved)
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding()
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(Theme.hairline).frame(height: 1)
        }
    }
}

#if DEBUG
    #Preview("Edit Metadata") {
        let seed = PreviewData.releaseEditSeed(trackCount: 6)
        EditMetadataSheet(
            releaseId: "release-preview",
            seed: seed,
            onSave: { _ in },
            onReset: {
                seed.edit
            },
            onSaved: {},
            onCancel: {}
        )
        .frame(width: 1_280, height: 900)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(ImageStore.stub())
        .environment(UiStore())
        .environment(ReleaseEditor.stub())
        .preferredColorScheme(.dark)
    }
#endif
