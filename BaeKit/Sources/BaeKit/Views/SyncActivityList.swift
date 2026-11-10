#if os(macOS)
    import SwiftUI

    /// What sync is doing on this device, as the title bar's indicator opens it:
    /// what needs the person first, then what is moving, and Retry now while
    /// core offers it, when a retry can clear a failure. Its sections go
    /// inside the platform's list or form.
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
            Section {
                let retry =
                    store.activity?.retryOffered == true
                    ? ErrorAction(
                        verbatim: localizedCoreString("core.activity.retry_now")
                    ) {
                        Task { await store.retryNow() }
                    }
                    : nil
                if let error = store.error {
                    Notice(error: error, action: retry)
                }
                else if store.activity?.retryOffered == true {
                    Button(localizedCoreString("core.activity.retry_now")) {
                        Task { await store.retryNow() }
                    }
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
                    if let failure = activity.failure,
                        let displayed = DisplayError(failure)
                    {
                        Notice(error: displayed)
                    }
                    PendingRecordsView(
                        records: activity.attention,
                        perform: nil
                    )
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

    /// A pass running, the upload queue and the download queue: one row each
    /// while the queue holds work, with what holds the queue back in the place
    /// of the bytes.
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
                    UploadsLine(
                        title: QueueSummary.line(uploads.summaryParts),
                        hold: uploads.wait?.text
                    )
                } icon: {
                    Image(systemName: "arrow.up.circle")
                }
            }
            if let downloads = activity.downloads {
                Label {
                    VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                        Text(QueueSummary.line(downloads.summaryParts))
                        if let hold = downloads.wait?.text {
                            Text(hold)
                                .themeText(.detail)
                                .foregroundStyle(.secondary)
                        }
                    }
                } icon: {
                    Image(systemName: "arrow.down.circle")
                }
            }
        }
    }

    /// The uploads' line, and under it what holds the queue back, or the
    /// bytes the files being sent have moved together, which a native label
    /// reads from the latest total each frame while files are being sent.
    private struct UploadsLine: View {
        let title: String
        let hold: String?

        @Environment(OutboxStore.self)
        private var outboxStore

        var body: some View {
            VStack(alignment: .leading, spacing: ThemeSpace.inline) {
                Text(title)
                if let hold {
                    Text(hold)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
                else {
                    UploadTotalBytesText(total: outboxStore.totalProgress)
                }
            }
        }
    }
#endif
