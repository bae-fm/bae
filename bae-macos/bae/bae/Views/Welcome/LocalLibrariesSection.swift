import BaeKit
import SwiftUI

/// Libraries on this device in a box of rows: a click opens one, and its ⋯
/// menu, or a right click, holds what finishes the reset or restore of its
/// copy or its removal that stopped partway, then Details for one that opens,
/// then Remove, or Delete for one that never synced. Under the box, Add library… creates one, or opens the
/// code screen to scan a code or enter one.
struct LocalLibrariesSection: View {
    let libraries: [BridgeLibrary]
    let disabled: Bool
    let canDeleteActiveLibrary: Bool
    /// The removal running from here, with its steps.
    let removing: RunningRemoval?
    /// The library that isn't open which counting or removing it is
    /// opening now.
    let opening: ClosedLibraryOpening?
    /// A failed open of one of `libraries`, shown joined under its row.
    var openFailure: LoadError?
    /// The library whose unfinished join or restore on this Mac can't be
    /// read, as a row after the libraries.
    var unreadableInstallation: String?
    let onDiscardInstallation: (String) -> Void
    let onResetFromCloud: (BridgeLibrary) -> Void
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onFinishRemoving: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void
    /// Reads how a library stands with its storage, for its details.
    let readStanding:
        @Sendable (_ libraryId: String) async throws ->
            BridgeLibraryStanding
    let onCreate: () -> Void
    let onScanCode: () -> Void
    let onEnterCode: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.compact) {
            Text("Your libraries")
                .themeText(.strong)
                .foregroundStyle(.secondary)
                .padding(.horizontal, ThemeSpace.inline)
            VStack(spacing: 0) {
                ForEach(
                    Array(libraries.enumerated()),
                    id: \.element.id
                ) { index, library in
                    if index > 0 {
                        Divider()
                            .padding(.horizontal, ThemeSpace.group)
                            .opacity(0.6)
                    }
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
                        readStanding: readStanding,
                    )
                    if let openFailure, openFailure.libraryId == library.id {
                        OpenFailureNotice(
                            failure: openFailure,
                            onResetFromCloud: { onResetFromCloud(library) }
                        )
                    }
                }
                if let unreadableInstallation {
                    if !libraries.isEmpty {
                        Divider()
                            .padding(.horizontal, ThemeSpace.group)
                            .opacity(0.6)
                    }
                    UnreadableInstallationRow(
                        libraryId: unreadableInstallation,
                        disabled: disabled,
                        onDiscard: onDiscardInstallation
                    )
                }
            }
            .card()
            MacSheetFooter {
                MacMenuButton(bordered: true) {
                    MacMenuItem("Create new library…", action: onCreate)
                    MacMenuItem("Scan code…", action: onScanCode)
                    MacMenuItem("Enter code…", action: onEnterCode)
                } label: {
                    Text("Add library…")
                }
                .fixedSize()
                .disabled(disabled)
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth)
    }
}

/// An unfinished join or restore this Mac can't read: its library's id under
/// the name of what it is, with Discard… to delete it. It can't open or
/// resume, so nothing else is offered.
private struct UnreadableInstallationRow: View {
    let libraryId: String
    let disabled: Bool
    let onDiscard: (String) -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text("Unfinished join that cannot be read")
                    .themeText(.strong)
                    .foregroundStyle(Theme.textPrimary)
                Text(verbatim: libraryId)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: ThemeSpace.group)
            Button("Discard…") { onDiscard(libraryId) }
                .buttonStyle(.accentBordered)
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.vertical, ThemeSpace.related + ThemeSpace.line)
        .frame(maxWidth: .infinity, alignment: .leading)
        .disabled(disabled)
    }
}

/// Why a library failed to open, joined to its row with no line between
/// them (the row names the library): the title and Details, and for a failure
/// in this Mac's data for it (core says a reset from the cloud replaces what
/// failed), what is wrong and Reset from cloud….
private struct OpenFailureNotice: View {
    let failure: LoadError
    let onResetFromCloud: () -> Void

    var body: some View {
        Notice(
            title: failure.step.title,
            advice: failure.error.resetFromCloudReplaces
                ? String(
                    localized: "Something is wrong with this library’s data."
                ) : nil,
            error: failure.error,
            action: failure.error.resetFromCloudReplaces
                ? ErrorAction("Reset from cloud…", perform: onResetFromCloud)
                : nil,
            buttons: .below
        )
        .padding(.horizontal, ThemeSpace.group)
        .padding(.bottom, ThemeSpace.group)
    }
}

/// One library row: its name, and under it why the library doesn't open as
/// it is, when it doesn't: its settings can't be read, a reset or restore of
/// its copy stopped partway, or its removal did. A click opens a library
/// that opens. The ⋯ menu offers to finish what stopped, then Details for a
/// library that opens, which shows in a popover off the ⋯, then Remove, or
/// Delete for a library that doesn't sync. While counting or removing opens
/// the library, a step the open runs shows under the name, and then a step
/// the removal runs, as its name alone since the spinner in place of the ⋯
/// shows that it runs; a library that can't open shows the removal's steps
/// alone.
private struct LibraryRow: View {
    let library: BridgeLibrary
    let disabled: Bool
    let deleteDisabled: Bool
    let openSteps: LatestSource<BridgeOpenProgress>?
    let removalSteps: LatestSource<BridgeClosingProgress>?
    let onOpen: (BridgeLibrary) -> Void
    let onFinish: (BridgeLibrary, BridgeReplacedBy) -> Void
    let onFinishRemoving: (BridgeLibrary) -> Void
    let onRemove: (BridgeLibrary) -> Void
    let readStanding:
        @Sendable (_ libraryId: String) async throws ->
            BridgeLibraryStanding
    /// Whether the details popover shows, hung off the ⋯ button.
    @State
    private var showsDetails = false

    var body: some View {
        HStack(spacing: ThemeSpace.group) {
            label
            if removalSteps != nil || openSteps != nil {
                MacSpinner()
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
            }
            else {
                moreMenu
            }
        }
        .padding(.horizontal, ThemeSpace.group)
        .padding(.vertical, ThemeSpace.related + ThemeSpace.line)
        .frame(maxWidth: .infinity, alignment: .leading)
        .macContextMenu { menuItems }
        .disabled(disabled)
    }

    /// The name and what's under it; the whole of it, empty space too, opens a
    /// library that opens.
    @ViewBuilder
    private var label: some View {
        let content = HStack(spacing: ThemeSpace.group) {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(library.title)
                    .themeText(.strong)
                    .foregroundStyle(Theme.textPrimary)
                MacProgressContainer(steps: removalSteps, look: .note) {
                    MacProgressContainer(steps: openSteps, look: .note) {
                        if let line = library.stateLine {
                            Text(line)
                                .themeText(.detail)
                                .foregroundStyle(
                                    isUnreadable
                                        ? StatusTone.danger.color
                                        : Color.secondary
                                )
                        }
                    }
                }
            }
            Spacer(minLength: ThemeSpace.group)
        }
        .contentShape(Rectangle())
        if library.isOpenable {
            content
                .onTapGesture { onOpen(library) }
                .accessibilityAddTraits(.isButton)
                .accessibilityAction { onOpen(library) }
        }
        else {
            content
        }
    }

    private var isUnreadable: Bool {
        if case .unreadable = library.state { return true }
        return false
    }

    private var moreMenu: some View {
        MacMoreMenu { menuItems }
            .macPopover(isPresented: $showsDetails, arrowEdge: .bottom) {
                WelcomeLibraryDetails(
                    library: library,
                    readStanding: readStanding
                )
            }
    }

    private var menuItems: [MacMenuEntry] {
        var entries: [MacMenuEntry] = []
        switch library.state {
        case .replacementUnfinished(let by, _):
            entries.append(
                .item(
                    MacMenuItem(finishTitle(by)) { onFinish(library, by) }
                )
            )
        case .deletionUnfinished:
            entries.append(
                .item(
                    MacMenuItem("Finish removing") {
                        onFinishRemoving(library)
                    }
                )
            )
        case .openable:
            entries.append(
                .item(MacMenuItem("Details…") { showsDetails = true })
            )
        case .unreadable, .nameUnreadable:
            break
        }
        entries.append(
            .item(
                MacMenuItem(
                    verbatim: LibraryRemovalConfirmation.Ending(of: library)
                        .menuItem,
                    disabled: deleteDisabled
                ) { onRemove(library) }
            )
        )
        return entries
    }

    private func finishTitle(_ by: BridgeReplacedBy) -> LocalizedStringResource
    {
        switch by {
        case .restore: "Finish restoring"
        case .reset: "Finish reset from cloud"
        }
    }
}
