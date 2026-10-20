import SwiftUI

/// What sync is doing on this device, as the title bar's indicator opens it:
/// what needs the person first, then what is moving, then what is still
/// arriving from other devices, and Retry Now. Its sections go inside the
/// platform's list or form.
public struct SyncActivityList: View {
    /// Opens the sync settings, where each record that needs the person
    /// offers its action.
    private let onOpenSettings: () -> Void
    /// Opens the offline downloads, where a platform lists them apart.
    private let onOpenDownloads: (() -> Void)?

    @Environment(SyncActivityStore.self)
    private var store

    public init(
        onOpenSettings: @escaping () -> Void,
        onOpenDownloads: (() -> Void)? = nil
    ) {
        self.onOpenSettings = onOpenSettings
        self.onOpenDownloads = onOpenDownloads
    }

    public var body: some View {
        if let activity = store.activity {
            SyncActivitySections(
                activity: activity,
                onOpenSettings: onOpenSettings,
                onOpenDownloads: onOpenDownloads
            )
        }
        else if store.error == nil {
            ProgressView()
                .controlSize(.small)
        }
        Section {
            if let error = store.error {
                ErrorDetailDisclosure(error: error)
            }
            Button(localizedCoreString("core.activity.retry_now")) {
                store.retryNow()
            }
        }
    }
}

private struct SyncActivitySections: View {
    let activity: BridgeSyncActivity
    let onOpenSettings: () -> Void
    let onOpenDownloads: (() -> Void)?

    var body: some View {
        if activity.needsAttention {
            Section(localizedCoreString("core.activity.attention")) {
                if let failure = activity.failure {
                    ErrorDetailDisclosure(error: DisplayError(failure))
                }
                PendingRecordsView(records: activity.attention, perform: nil)
                Button(
                    localizedCoreString("core.activity.open_settings"),
                    action: onOpenSettings
                )
            }
        }
        Section {
            TransferRows(activity: activity, onOpenDownloads: onOpenDownloads)
        }
        ArrivalSections(arrivals: activity.arrivals)
        if !activity.moving, !activity.needsAttention, !activity.syncing {
            Section {
                Text(localizedCoreString("core.activity.idle"))
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// A pass running, the files being sent, the artwork and the offline copies
/// downloading: one row each while it moves.
private struct TransferRows: View {
    let activity: BridgeSyncActivity
    let onOpenDownloads: (() -> Void)?

    @Environment(SyncActivityStore.self)
    private var store

    var body: some View {
        if activity.syncing {
            Label(
                localizedCoreString("core.activity.syncing"),
                systemImage: "arrow.triangle.2.circlepath"
            )
        }
        if let uploads = activity.uploads {
            Label {
                TransferLine(
                    title: QueueSummary.line(uploads.summaryParts),
                    done: uploads.bytesDone,
                    total: uploads.bytesTotal
                )
            } icon: {
                Image(systemName: "arrow.up.circle")
            }
        }
        if let artwork = activity.artwork {
            Label {
                HStack {
                    TransferLine(
                        title: localizedCoreString(
                            "core.artwork_cache.downloading"
                        ),
                        done: artwork.bytesCached,
                        total: artwork.bytesTotal
                    )
                    Spacer()
                    Button("Cancel") {
                        Task { await store.cancelArtwork() }
                    }
                    .buttonStyle(.borderless)
                }
            } icon: {
                Image(systemName: "photo.on.rectangle")
            }
        }
        if let downloads = activity.downloads {
            let row = Label(
                QueueSummary.line(downloads.summaryParts),
                systemImage: "arrow.down.circle"
            )
            if let onOpenDownloads {
                Button(action: onOpenDownloads) { row }
            }
            else {
                row
            }
        }
    }
}

/// A transfer's line, and the bytes it has moved when it counts any.
private struct TransferLine: View {
    let title: String
    let done: UInt64
    let total: UInt64

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.inline) {
            Text(title)
            if total > 0 {
                Text(formattedDownloadBytesProgress(done: done, total: total))
                    .themeText(.detail)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// The files still arriving, one section per device they come from: the
/// albums by title, then a count of other pictures.
private struct ArrivalSections: View {
    let arrivals: BridgeArrivals

    var body: some View {
        switch arrivals {
        case .loading:
            EmptyView()
        case .failed(let error):
            Section {
                ErrorDetailDisclosure(error: DisplayError(error))
            }
        case .ready(let devices, _, _):
            ForEach(devices, id: \.deviceName) { device in
                Section(BridgeArrivals.fromLine(device.deviceName)) {
                    ForEach(device.releases, id: \.releaseId) { release in
                        Text(release.albumTitle)
                            .foregroundStyle(.secondary)
                    }
                    if device.otherFiles > 0 {
                        Text(
                            String.localizedStringWithFormat(
                                localizedCoreString(
                                    "core.arrival.other_pictures"
                                ),
                                Int(device.otherFiles)
                            )
                        )
                        .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }
}

extension BridgeArrivals {
    /// "Arriving from <device>", the quiet line for files another device is
    /// still uploading.
    public static func fromLine(_ deviceName: String) -> String {
        String(
            format: localizedCoreString("core.arrival.from"),
            locale: Locale.current,
            deviceName
        )
    }
}
