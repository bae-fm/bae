import BaeKit
import SwiftUI

/// Libraries on this device, each with Open, or with what finishes the reset
/// or restore of its copy or its removal that stopped partway, and Remove.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let canDeleteActiveLibrary: Bool
    /// The removal running from here, with its steps.
    let removing: RunningRemoval?
    /// The library that isn't open which counting or removing it is
    /// opening now.
    let opening: ClosedLibraryOpening?
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onFinishRemoving: (BridgeLibrary) -> Void
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
                    openSteps: opening?.libraryId == library.id
                        ? opening?.progress.steps : nil,
                    removalSteps: removing?.libraryId == library.id
                        ? removing?.steps.steps : nil,
                    onOpen: onOpen,
                    onFinish: onFinish,
                    onFinishRemoving: onFinishRemoving,
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
/// and offers to finish it in place of Open; one whose removal stopped
/// partway says so, and offers to finish removing it. A library that doesn't
/// sync says it isn't synced and offers Delete in place of Remove. While counting or removing it
/// opens the library, a step the open runs shows in place of the detail line,
/// and then a step the removal runs; a library that can't open shows the
/// removal's steps alone.
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let deleteDisabled: Bool
    let openSteps: LatestSource<BridgeOpenProgress>?
    let removalSteps: LatestSource<BridgeClosingStep>?
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onFinishRemoving: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(library.name)
                    .themeText(.rowTitle)
                // Every row keeps the detail line so rows share one height.
                StepProgressLine(steps: removalSteps) {
                    StepProgressLine(steps: openSteps) {
                        Text(detailLine)
                            .themeText(.detail)
                            .foregroundStyle(
                                library.isOpenable
                                    ? Color.secondary
                                    : StatusTone.danger.color
                            )
                    }
                }
            }
            Spacer(minLength: ThemeSpace.group)
            Button(primaryTitle, action: primaryAction)
                .buttonStyle(.bordered)
                .disabled(!canAct)
            Button(
                LibraryRemovalConfirmation.Ending(of: library).action,
                systemImage: "trash",
                role: .destructive
            ) {
                onRemove(library)
            }
            .buttonStyle(.bordered)
            .disabled(deleteDisabled)
        }
        .padding(.horizontal, ThemeSpace.edge)
        .padding(.vertical, ThemeSpace.related)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.secondary.opacity(ThemeOpacity.tint))
        .clipShape(RoundedRectangle(cornerRadius: ThemeRadius.control))
        .disabled(disabled)
    }

    /// The line under the name; a library that doesn't sync says it isn't
    /// synced.
    private var detailLine: String {
        if case .openable(nil) = library.state {
            return String(localized: "Not synced")
        }
        return library.detailLine
    }

    private var primaryTitle: LocalizedStringKey {
        switch library.state {
        case .openable, .unreadable: "Open"
        case .replacementUnfinished(.restore): "Finish restoring"
        case .replacementUnfinished(.reset): "Finish resetting"
        case .deletionUnfinished: "Finish removing"
        }
    }

    /// Open, or finish the reset, restore or removal that stopped; a library
    /// whose settings can't be read offers none of them.
    private var canAct: Bool {
        if case .unreadable = library.state { return false }
        return true
    }

    private func primaryAction() {
        switch library.state {
        case .openable, .unreadable: onOpen(library)
        case .replacementUnfinished(let by): onFinish(library, by)
        case .deletionUnfinished: onFinishRemoving(library)
        }
    }
}

#if DEBUG
    #Preview {
        LocalLibrariesSection(
            libraries: PreviewData.welcomeLibraries,
            disabled: false,
            canDeleteActiveLibrary: true,
            removing: nil,
            opening: nil,
            onOpen: { _ in },
            onFinish: { _, _ in },
            onFinishRemoving: { _ in },
            onRemove: { _ in },
        )
        .padding()
    }
#endif
