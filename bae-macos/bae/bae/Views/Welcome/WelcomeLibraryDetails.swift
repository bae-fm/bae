import BaeKit
import SwiftUI

/// A library in a popover off its row, its name the title of its one
/// section: how it stands with its storage, when it last synced, and where it
/// is stored. The bridge answers from the open library's live sync status, or
/// for one that isn't open reads it as this Mac last knew it without opening
/// it. A library that never synced says so and has nothing more to show.
struct WelcomeLibraryDetails: View {
    let library: BridgeLibrary
    /// Reads how a library stands with its storage, by its id.
    let readStanding:
        @Sendable (_ libraryId: String) async throws ->
            BridgeLibraryStanding
    /// The library's standing, read as the popover appears; only a
    /// library with storage has one.
    @State
    private var status: SectionLoad<BridgeLibraryStanding> = .loading
    @Environment(\.clockNow)
    private var clockNow

    private var storage: BridgeStorageLocation? {
        if case .openable(let storage) = library.state { return storage }
        return nil
    }

    /// The popover's own inset, as the boards draw it: 14 at the sides and
    /// bottom, and the title 18 down.
    private let popoverInset: CGFloat = 14

    var body: some View {
        SettingsTitledBox(title: Text(verbatim: library.title)) {
            rows
        }
        .padding(.horizontal, popoverInset)
        .padding(.top, popoverInset + ThemeSpace.inline)
        .padding(.bottom, popoverInset)
        .frame(width: 348)
        .background(Theme.surfaceElevated)
        .task { await load() }
    }

    @ViewBuilder
    private var rows: some View {
        if let storage {
            switch status {
            case .loading:
                MacSpinner()
            case .failed(let failure):
                MacNotice(error: failure)
            case .loaded(let status):
                MacRow(
                    "Status",
                    value: status.statusLine,
                    tone: status.statusTone
                )
                if let lastSynced = status.lastSynced {
                    MacRow(
                        "Last synced",
                        value: lastSynced.agoLine(now: clockNow ?? Date())
                    )
                }
            }
            MacCloudHome(location: storage, shown: .bare)
        }
        else {
            MacRow("Status", value: String(localized: "Not synced"))
        }
    }

    /// Read the status of a library that has storage. A failure shows in
    /// place of its rows.
    private func load() async {
        guard storage != nil else { return }
        do {
            status = .loaded(try await readStanding(library.id))
        }
        catch is CancellationError {}
        catch {
            guard let failure = DisplayError(error) else { return }
            status = .failed(failure)
        }
    }
}

extension BridgeLibraryStanding {
    /// The state in the person's language: sync's state as the Sync page
    /// words it, or that this Mac holds changes it hasn't uploaded.
    fileprivate var statusLine: String {
        switch state {
        case .unsentChanges: String(localized: "Unsent changes")
        case .sync(let shown): shown.statusLine
        }
    }

    fileprivate var statusTone: StatusTone {
        switch state {
        case .unsentChanges: .warning
        case .sync(let shown): shown.statusTone
        }
    }
}
