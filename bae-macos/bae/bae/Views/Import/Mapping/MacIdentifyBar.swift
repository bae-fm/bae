import BaeKit
import SwiftUI

/// A command in the card's metadata menu. Each replaces what the person has,
/// so the card asks before running it.
enum ImportDraftCommand: CaseIterable, Identifiable {
    /// Replace the draft with what the candidate's own files say.
    case resetToFileMetadata
    case clearMetadata

    var id: Self { self }

    var title: String {
        switch self {
        case .resetToFileMetadata: String(localized: "Reset to file metadata")
        case .clearMetadata: String(localized: "Clear metadata")
        }
    }

    /// What the confirmation asks.
    var question: String {
        switch self {
        case .resetToFileMetadata: String(localized: "Reset to file metadata?")
        case .clearMetadata: String(localized: "Clear metadata?")
        }
    }

    /// What the confirmation says the command changes and keeps.
    var consequence: String {
        switch self {
        case .resetToFileMetadata:
            String(
                localized:
                    "Replace metadata with what the files, sheets and folder say about the release. Tracks and audio assignments will remain unchanged."
            )
        case .clearMetadata:
            String(
                localized:
                    "The candidate files and mapping choices will remain unchanged."
            )
        }
    }

    func run(with actions: ImportReleaseSourceActions) {
        switch self {
        case .resetToFileMetadata: actions.resetToFileMetadata()
        case .clearMetadata: actions.clearMetadata()
        }
    }
}

/// The bar of identify entries: the Identify label, Automatic and Search,
/// the Metadata menu where the caller offers its commands, and the caller's
/// own trailing controls at the far end.
struct MacIdentifyBar<Trailing: View>: View {
    /// The space between the identify group and the trailing controls.
    let spacing: CGFloat
    /// Whether a read or save in flight disables the identify controls.
    let identifyDisabled: Bool
    let onAutomatic: () -> Void
    let onSearch: () -> Void
    /// Runs the command the Metadata menu picks; `nil` draws no menu.
    let onMetadataCommand: ((ImportDraftCommand) -> Void)?
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        HStack(alignment: .center, spacing: spacing) {
            HStack(spacing: ThemeSpace.related) {
                Text("Identify")
                    .themeText(.strong)
                    .foregroundStyle(.secondary)
                Button("Automatic") { onAutomatic() }
                    .buttonStyle(.accentBordered)
                Button("Search") { onSearch() }
                    .buttonStyle(.accentBordered)
                if let onMetadataCommand {
                    metadataMenu(onMetadataCommand)
                }
            }
            .disabled(identifyDisabled)
            Spacer(minLength: ThemeSpace.group)
            trailing()
        }
    }

    /// Commands that replace the draft's metadata.
    private func metadataMenu(
        _ run: @escaping (ImportDraftCommand) -> Void
    ) -> some View {
        MacMenuButton(bordered: true) {
            for command in ImportDraftCommand.allCases {
                MacMenuItem(verbatim: command.title) {
                    run(command)
                }
            }
        } label: {
            Image(systemName: "ellipsis")
        }
        .accessibilityLabel(Text("Metadata"))
        .fixedSize()
    }
}
