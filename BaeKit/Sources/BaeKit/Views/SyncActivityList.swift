import SwiftUI

/// What sync is doing on this device, as the title bar's indicator opens it:
/// what needs the person first, then what is moving, and Retry Now. Its
/// sections go inside the platform's list or form.
public struct SyncActivityList: View {
    /// Opens the sync settings, where each record that needs the person
    /// offers its action.
    private let onOpenSettings: () -> Void

    @Environment(SyncActivityStore.self)
    private var store

    public init(onOpenSettings: @escaping () -> Void) {
        self.onOpenSettings = onOpenSettings
    }

    public var body: some View {
        if let activity = store.activity {
            SyncActivitySections(
                activity: activity,
                onOpenSettings: onOpenSettings
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
                Task { await store.retryNow() }
            }
        }
    }
}

private struct SyncActivitySections: View {
    let activity: BridgeSyncActivity
    let onOpenSettings: () -> Void

    var body: some View {
        if activity.needsAttention {
            Section(localizedCoreString("core.activity.attention")) {
                if let failure = activity.failure {
                    ErrorDetailDisclosure(error: DisplayError(failure))
                }
                PendingRecordsView(records: activity.attention, perform: nil)
                AbandonedAttachmentsView(attachments: activity.abandoned)
                Button(
                    localizedCoreString("core.activity.open_settings"),
                    action: onOpenSettings
                )
            }
        }
        Section {
            TransferRows(activity: activity)
        }
        if !activity.moving, !activity.needsAttention, !activity.syncing {
            Section {
                Text(localizedCoreString("core.activity.idle"))
                    .foregroundStyle(.secondary)
            }
        }
    }
}

/// A pass running, the files being sent and the offline copies downloading:
/// one row each while it moves.
private struct TransferRows: View {
    let activity: BridgeSyncActivity

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
        if let downloads = activity.downloads {
            Label(
                QueueSummary.line(downloads.summaryParts),
                systemImage: "arrow.down.circle"
            )
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
