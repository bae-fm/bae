import BaeKit
import SwiftUI

/// Libraries on this device, each with Open, or with what finishes the reset
/// or restore of its copy that stopped partway, and Remove.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let canDeleteActiveLibrary: Bool
    let removingLibraryId: String?
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            WelcomeSectionHeader(title: "Your libraries")
            ForEach(libraries, id: \.id) { library in
                LibraryRow(
                    library: library,
                    disabled: disabled,
                    deleteDisabled: library.isActive
                        && !canDeleteActiveLibrary,
                    isRemoving: removingLibraryId == library.id,
                    onOpen: onOpen,
                    onFinish: onFinish,
                    onRemove: onRemove,
                )
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
    }
}

/// One library row: its name, its provider and account when it has a cloud
/// home, Open and Remove. A library whose settings can't be read says why,
/// and doesn't open. One whose reset or restore stopped partway says which,
/// and offers to finish it in place of Open.
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let deleteDisabled: Bool
    let isRemoving: Bool
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(library.name)
                    .themeText(.rowTitle)
                // Every row keeps the detail line so rows share one height.
                Text(library.detailLine)
                    .themeText(.detail)
                    .foregroundStyle(
                        library.isOpenable
                            ? Color.secondary : StatusTone.danger.color
                    )
            }
            Spacer(minLength: ThemeSpace.group)
            Button(primaryTitle, action: primaryAction)
                .buttonStyle(.bordered)
                .disabled(!canAct)
            ZStack {
                Button("Remove", systemImage: "trash", role: .destructive) {
                    onRemove(library)
                }
                .buttonStyle(.bordered)
                .disabled(deleteDisabled)
                .opacity(isRemoving ? 0 : 1)
                .allowsHitTesting(!isRemoving)
                ProgressView()
                    .controlSize(.small)
                    .opacity(isRemoving ? 1 : 0)
            }
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
        .disabled(disabled)
    }

    private var primaryTitle: LocalizedStringKey {
        switch library.state {
        case .openable, .unreadable: "Open"
        case .replacementUnfinished(.restore): "Finish Restoring"
        case .replacementUnfinished(.reset): "Finish Resetting"
        }
    }

    /// Open, or finish the reset or restore that stopped; a library whose
    /// settings can't be read offers neither.
    private var canAct: Bool {
        if case .unreadable = library.state { return false }
        return true
    }

    private func primaryAction() {
        switch library.state {
        case .openable, .unreadable: onOpen(library)
        case .replacementUnfinished(let by): onFinish(library, by)
        }
    }
}

#if DEBUG
    #Preview {
        LocalLibrariesSection(
            libraries: PreviewData.welcomeLibraries,
            disabled: false,
            canDeleteActiveLibrary: true,
            removingLibraryId: nil,
            onOpen: { _ in },
            onFinish: { _, _ in },
            onRemove: { _ in },
        )
        .padding()
    }
#endif
