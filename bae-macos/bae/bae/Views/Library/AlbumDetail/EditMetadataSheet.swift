import BaeKit
import SwiftUI

/// Library presentation shell around the shared persisted-release editor.
struct EditMetadataSheet: View {
    let onCancel: @MainActor @Sendable () -> Void
    let onSaved: @MainActor @Sendable () -> Void

    @State
    private var session: ReleaseMetadataEditSession

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
                    ReleaseMetadataEditorContent(session: session)
                        .padding(24)
                }
                footer
            }
            .frame(width: size.width, height: size.height)
            .background(Theme.background)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onDisappear { session.cancelTasks() }
    }

    static func modalSize(in host: CGSize) -> CGSize {
        CGSize(
            width: min(host.width, min(1_200, max(760, host.width - 80))),
            height: min(host.height, min(860, max(600, host.height - 80)))
        )
    }

    var resetButtonIsVisible: Bool {
        session.canResetToSource
    }

    private var header: some View {
        HStack {
            Text("Edit Metadata").font(.headline)
            Spacer()
            Button("Cancel") { onCancel() }
                .keyboardShortcut(.cancelAction)
                .disabled(session.isBusy)
        }
        .padding()
    }

    private var footer: some View {
        VStack(spacing: 8) {
            if let message = session.validationMessage
                ?? session.failureMessage
            {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                    Text(message)
                }
                .font(.caption)
                .foregroundStyle(.red)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(spacing: 12) {
                Button("Reset to Source") { session.resetToSource() }
                    .disabled(session.isBusy)
                    .opacity(resetButtonIsVisible ? 1 : 0)
                    .allowsHitTesting(resetButtonIsVisible)
                Spacer()
                if session.isBusy {
                    ProgressView().controlSize(.small)
                    Text(session.isSaving ? "Saving..." : "Resetting...")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                else {
                    Button("Save") {
                        session.save(onSuccess: onSaved)
                    }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding()
        .background(Theme.surface)
        .overlay(alignment: .top) {
            Rectangle().fill(.white.opacity(0.08)).frame(height: 1)
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
            onReset: { seed.edit },
            onSaved: {},
            onCancel: {}
        )
        .frame(width: 1_280, height: 900)
        .environment(PreviewData.artistAssignmentsLibrary())
        .environment(ImageStore.stub())
        .environment(UiStore())
        .preferredColorScheme(.dark)
    }
#endif
