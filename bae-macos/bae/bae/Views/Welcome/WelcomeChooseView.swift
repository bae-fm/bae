import BaeKit
import SwiftUI

/// The first screen: open a library on this device, restore one from a code
/// saved in iCloud Keychain, create one, or scan or enter a code that joins
/// someone's library or adds this Mac to one of the person's own.
struct WelcomeChooseView: View {
    /// A failed library open or dev reset, shown as a callout under the
    /// tagline, titled by the step that failed.
    let loadError: LoadError?
    /// What launch found wrong, shown above `loadError` and never replaced by
    /// it.
    var launchProblems = LaunchProblems.none
    let canDeleteActiveLibrary: Bool
    let onOpenLibrary: (String) -> Void
    /// Finish the reset or restore of a library's copy on this Mac that
    /// stopped partway, with the one call that finishes it.
    let onFinishReplacement: (String, BridgeReplacedBy) -> Void
    /// Replace this Mac's copy of a library from the cloud: opens the reset
    /// for that library.
    let onResetFromCloud: (String) -> Void
    /// Go to the step that asks the creator's name and creates a library.
    let onCreate: () -> Void
    let onRestoreSaved: (BridgeSavedRestoreCode) -> Void
    let onScanCode: () -> Void
    let onEnterCode: () -> Void

    @Environment(LibrarySetup.self)
    private var setup
    @Environment(LibraryRemovals.self)
    private var removals

    /// Libraries on this Mac and the restore codes saved for others, read on
    /// appear.
    @State
    private var listings = WelcomeListings()
    /// The removal's confirmation and failure, which the flow waits on.
    @State
    private var asker = AlertAsker()

    private var discoveredLibraries: [BridgeLibrary] {
        listings.localLibraries.value ?? []
    }

    /// The buttons stand on their own when the listing read no libraries or
    /// failed; once it lists some they are the items of the list's Add
    /// library…, and nothing shows while it loads.
    private var showsFirstRunActions: Bool {
        switch listings.localLibraries {
        case .loading: false
        case .failed: true
        case .loaded(let libraries): libraries.isEmpty
        }
    }

    private var isBusy: Bool {
        removals.removing != nil || removals.isRetrying
    }

    var body: some View {
        // Scrolls only when the content outgrows the window; the min-height
        // keeps it centered otherwise.
        GeometryReader { geometry in
            ScrollView {
                content
                    .frame(
                        maxWidth: .infinity,
                        minHeight: geometry.size.height
                    )
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .task { await listings.load(from: setup) }
        .presentsAlerts(of: asker)
    }

    /// Read what removing `library` would lose, ask, and remove it. A failure
    /// of the count or of the removal is offered back as an alert.
    private func askToRemove(_ library: BridgeLibrary) async {
        guard let loss = await removals.loss(closedLibrary: library) else {
            return await offerRetry()
        }
        guard
            await asker.ask(
                LibraryRemovalAlerts.confirmation(for: library, losing: loss)
            ) == 0
        else { return }
        await listings.remove(
            closedLibrary: library,
            losing: loss,
            with: removals,
            from: setup
        )
        await offerRetry()
    }

    /// Finish removing `library`, whose removal stopped partway.
    private func finishRemoving(_ library: BridgeLibrary) async {
        await listings.finishRemoving(library, with: removals, from: setup)
        await offerRetry()
    }

    /// When a removal failed, say so, and on Retry run again what failed.
    private func offerRetry() async {
        guard let failed = removals.failed else { return }
        let retry = await asker.ask(LibraryRemovalAlerts.failure(failed))
        removals.dismissFailure()
        guard failed.error.offersRetry, retry == 0 else { return }
        switch failed.retry {
        case .ask:
            await askToRemove(failed.library)
        case .remove(let loss):
            await listings.remove(
                closedLibrary: failed.library,
                losing: loss,
                with: removals,
                from: setup
            )
            await offerRetry()
        case .finish:
            await finishRemoving(failed.library)
        }
    }

    /// The failed open that shows under its library's row, once the list holds
    /// that library; any other failure shows under the tagline.
    private var openFailureInList: LoadError? {
        guard
            let loadError,
            loadError.step == .libraryOpen,
            let libraryId = loadError.libraryId,
            discoveredLibraries.contains(where: { $0.id == libraryId })
        else { return nil }
        return loadError
    }

    /// The gap under the logo block, which the boards draw as 40pt.
    private static let belowLogo = ThemeSpace.page + ThemeSpace.related

    private var content: some View {
        VStack(spacing: Self.belowLogo) {
            Spacer()
            VStack(
                spacing: showsFirstRunActions
                    ? ThemeSpace.group : ThemeSpace.related + ThemeSpace.line
            ) {
                Logo(
                    size: showsFirstRunActions ? 120 : 72,
                    bob: true,
                    glow: true
                )
                Text("Your music, everywhere")
                    .themeText(.heading)
                    .fontWeight(.regular)
                    .foregroundStyle(.secondary)
            }
            LaunchProblemNotices(launchProblems)
                .frame(maxWidth: WelcomeLayout.columnWidth)
            if let loadError, openFailureInList == nil {
                Notice(
                    title: loadError.step.title,
                    advice: loadError.step.advice,
                    error: loadError.error
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            if let unfinishedError = removals.unfinishedError {
                UnfinishedRemovalNotice(
                    error: unfinishedError,
                    removals: removals
                )
            }
            if let failure = listings.localLibraries.failure {
                Notice(
                    title: String(
                        localized: "Could not list the libraries on this Mac"
                    ),
                    advice: nil,
                    error: failure
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            else if !discoveredLibraries.isEmpty {
                LocalLibrariesSection(
                    libraries: discoveredLibraries,
                    disabled: isBusy,
                    canDeleteActiveLibrary: canDeleteActiveLibrary,
                    removing: removals.removing,
                    opening: removals.opening,
                    openFailure: openFailureInList,
                    onResetFromCloud: { onResetFromCloud($0.id) },
                    onOpen: { onOpenLibrary($0.id) },
                    onFinish: { onFinishReplacement($0.id, $1) },
                    onFinishRemoving: { library in
                        Task { await finishRemoving(library) }
                    },
                    onRemove: { library in
                        Task { await askToRemove(library) }
                    },
                    readStanding: setup.libraryStanding,
                    onCreate: onCreate,
                    onScanCode: onScanCode,
                    onEnterCode: onEnterCode
                )
            }
            if let failure = listings.savedCodes.list.failure {
                Notice(
                    title: String(
                        localized:
                            "Could not read restore codes from your keychain"
                    ),
                    advice: nil,
                    error: failure
                )
                .frame(maxWidth: WelcomeLayout.columnWidth)
            }
            else if listings.savedCodes.list.value?.isEmpty == false {
                if let deleteError = listings.savedCodes.deleteError {
                    Notice(
                        title: String(
                            localized: "Could not delete the restore code"
                        ),
                        advice: nil,
                        error: deleteError
                    )
                    .frame(maxWidth: WelcomeLayout.columnWidth)
                }
                KeychainRestoreContainer(
                    codes: listings.savedCodes,
                    disabled: isBusy,
                    onRestore: { onRestoreSaved($0.code) }
                )
            }
            if showsFirstRunActions {
                firstRunActions
            }
            Spacer()
        }
        .padding()
    }

    /// The Create button's label.
    private var createButtonLabel: some View {
        Text("Create new library…")
            .frame(maxWidth: .infinity)
    }

    /// The width the first-run buttons share.
    private static let firstRunButtonWidth: CGFloat = 240

    /// First run: Create as the default action, then Scan code and Enter
    /// code side by side.
    private var firstRunActions: some View {
        VStack(spacing: ThemeSpace.related + ThemeSpace.line) {
            Button(action: onCreate) { createButtonLabel }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(isBusy)
                .keyboardShortcut(.defaultAction)
            HStack(spacing: ThemeSpace.related) {
                Button(action: onScanCode) {
                    Text("Scan code").frame(maxWidth: .infinity)
                }
                Button(action: onEnterCode) {
                    Text("Enter code").frame(maxWidth: .infinity)
                }
            }
            .buttonStyle(SecondaryButtonStyle().accentFocus())
            .disabled(isBusy)
        }
        .controlSize(.large)
        .frame(width: Self.firstRunButtonWidth)
    }
}

/// The choose screen's shared column width.
enum WelcomeLayout {
    static let columnWidth: CGFloat = 420
}

/// Why a removal left this Mac's files for a library it already left, or why
/// reading or finishing that failed, with Retry while there is something to
/// finish.
private struct UnfinishedRemovalNotice: View {
    let error: DisplayError
    let removals: LibraryRemovals

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Notice(
                title: String(localized: "Could not remove the library"),
                advice: nil,
                error: error,
                action: removals.unfinished == nil
                    ? nil
                    : ErrorAction(isDisabled: removals.isRetrying) {
                        removals.retryUnfinished()
                    }
            )
            if let finishing = removals.finishing {
                MacProgressContainer(steps: finishing.steps) {
                    EmptyView()
                }
            }
        }
        .frame(maxWidth: WelcomeLayout.columnWidth, alignment: .leading)
    }
}
