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

    /// A pass running, then the Uploading, Pinning and Making local lines,
    /// each while its batch holds a release, counting its releases done or
    /// showing what holds its queue back in the count's place.
    private struct TransferRows: View {
        let activity: BridgeSyncActivity

        var body: some View {
            if activity.syncing {
                Text(localizedCoreString("core.activity.syncing"))
            }
            if let uploads = activity.uploads {
                TransferLine(
                    title: localizedCoreString("core.activity.uploading"),
                    count: uploads.wait?.text ?? uploads.batch.text
                )
            }
            if let pins = activity.pins {
                TransferLine(
                    title: localizedCoreString("core.activity.pinning"),
                    count: pins.wait?.text ?? pins.batch.text
                )
            }
            if let makingLocal = activity.makingLocal {
                TransferLine(
                    title: localizedCoreString("core.activity.making_local"),
                    count: makingLocal.wait?.text ?? makingLocal.batch.text
                )
            }
        }
    }

    /// One transfer line: its words, and at the end how many of its releases
    /// are done, or what holds its queue back.
    private struct TransferLine: View {
        let title: String
        let count: String

        var body: some View {
            HStack(alignment: .firstTextBaseline, spacing: ThemeSpace.related) {
                Text(title)
                Spacer(minLength: 0)
                Text(count)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
    }
#endif
