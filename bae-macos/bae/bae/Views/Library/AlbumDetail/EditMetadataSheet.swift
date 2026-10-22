import BaeKit
import SwiftUI

/// Library presentation shell around the shared persisted-release editor.
struct EditMetadataSheet: View {
    let onCancel: @MainActor @Sendable () -> Void
    /// The save landed; receives the album the release is on after it.
    let onSaved: @MainActor @Sendable (String) -> Void
    let onViewAlbum: (String) -> Void

    @State
    private var session: ReleaseMetadataEditSession
    @State
    private var showingCoverPicker = false
    @Environment(ReleaseEditor.self)
    private var releaseEditor

    init(
        releaseId: String,
        seed: BridgeReleaseEditSeed,
        commands: ReleaseEditCommands,
        onSaved: @escaping @MainActor @Sendable (String) -> Void,
        onViewAlbum: @escaping (String) -> Void,
        onCancel: @escaping @MainActor @Sendable () -> Void
    ) {
        self.onCancel = onCancel
        self.onSaved = onSaved
        self.onViewAlbum = onViewAlbum
        _session = State(
            initialValue: ReleaseMetadataEditSession(
                releaseId: releaseId,
                seed: seed,
                commands: commands
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
                        onEditCover: { showingCoverPicker = true },
                        onViewAlbum: onViewAlbum
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
                        try await session.refreshCover()
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
            if let shown = session.validationMessage.map({
                DisplayError(line: $0)
            })
                ?? session.failure
            {
                ErrorDetailDisclosure(error: shown)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(spacing: ThemeSpace.group) {
                Spacer()
                if session.isBusy {
                    ProgressView().controlSize(.small)
                    Text("Saving...")
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
            commands: PreviewData.releaseEditCommands(seed: seed),
            onSaved: { _ in },
            onViewAlbum: { _ in },
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
