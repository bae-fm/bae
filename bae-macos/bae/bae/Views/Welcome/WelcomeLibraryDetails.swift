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

    private var storage: BridgeStorageLocation? {
        if case .openable(let storage) = library.state { return storage }
        return nil
    }

    var body: some View {
        Form {
            Section(library.name) {
                rows
            }
        }
        .formStyle(.grouped)
        .scrollDisabled(true)
        .frame(width: 320)
        .task { await load() }
    }

    @ViewBuilder
    private var rows: some View {
        if let storage {
            switch status {
            case .loading:
                ProgressView()
                    .controlSize(.small)
            case .failed(let failure):
                Notice(error: failure)
            case .loaded(let status):
                LabeledContent("Status") {
                    HStack(spacing: ThemeSpace.related) {
                        Circle()
                            .fill(status.statusTone.color)
                            .frame(width: 7, height: 7)
                        Text(status.statusLine)
                            .foregroundStyle(status.statusTone.color)
                    }
                }
                if let lastSynced = status.lastSynced {
                    LabeledContent("Last synced", value: lastSynced.agoLine)
                }
            }
            CloudProviderConnectedSection(location: storage)
        }
        else {
            LabeledContent("Status", value: String(localized: "Not synced"))
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

#if DEBUG
    #Preview("Synced") {
        PreviewScenes.welcomeLibraryDetails(
            PreviewData.homeMusic,
            readStanding: { _ in PreviewData.standingSynced }
        )
    }

    #Preview("Unsent changes") {
        PreviewScenes.welcomeLibraryDetails(
            PreviewData.homeMusic,
            readStanding: { _ in PreviewData.standingUnsent }
        )
    }
#endif
