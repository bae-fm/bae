import BaeKit
import SwiftUI

/// The library's presentation of the one release editor, opened by the album
/// detail's Edit… item.
struct ReleaseEditorSheet: View {
    let displayName: String
    let onClose: @MainActor @Sendable () -> Void
    /// The save landed; receives the album the release is on after it.
    let onSaved: @MainActor @Sendable (String) -> Void

    @State
    private var session: ReleaseMetadataEditSession

    init(
        releaseId: String,
        displayName: String,
        opened: OpenedReleaseEdit,
        commands: ReleaseEditCommands,
        onSaved: @escaping @MainActor @Sendable (String) -> Void,
        onClose: @escaping @MainActor @Sendable () -> Void
    ) {
        self.displayName = displayName
        self.onClose = onClose
        self.onSaved = onSaved
        _session = State(
            initialValue: ReleaseMetadataEditSession(
                releaseId: releaseId,
                opened: opened,
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
                    ReleaseEditorView(
                        session: session,
                        displayName: displayName,
                        onSaved: onSaved
                    )
                    .padding(ThemeSpace.section)
                }
            }
            .frame(width: size.width, height: size.height)
            .background(Theme.background)
            .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.card))
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        // When someone deletes the release while its editor is open, the
        // sheet closes.
        .onChange(of: session.releaseGone) { _, gone in
            if gone { onClose() }
        }
        // However the sheet goes, its work stops; what it did not save goes
        // with its session.
        .onDisappear { session.cancelTasks() }
    }

    /// How much narrower and shorter than its host the sheet sits.
    nonisolated private static let hostMargin: CGFloat = 80

    nonisolated static func modalSize(in host: CGSize) -> CGSize {
        CGSize(
            width: min(
                host.width,
                min(1_100, max(760, host.width - hostMargin))
            ),
            height: min(
                host.height,
                min(1_000, max(600, host.height - hostMargin))
            )
        )
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text("Edit release")
                    .themeText(.heading)
                Text(displayName)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Button("Close") { onClose() }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.vertical, ThemeSpace.group + ThemeSpace.line)
        .padding(.horizontal, ThemeSpace.edge)
    }
}
