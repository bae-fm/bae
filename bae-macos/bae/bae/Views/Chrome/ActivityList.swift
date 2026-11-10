import BaeKit
import SwiftUI

/// What bae needs the person for and what it is moving, as the Activity
/// panel lists it: each problem not dismissed first, a quiet row with an ×
/// whose click opens its details, then the transfer lines, or that everything
/// is up to date. Its sections go inside the caller's grouped form.
struct ActivityList: View {
    let problems: ActivityProblems
    /// The app's own problems, which `problems` lists by kind.
    let app: [AppProblem]
    @Binding
    var dismissed: Set<ActivityProblemID>
    /// Opens the sync settings, where each record that needs the person
    /// offers its action.
    let onOpenSettings: () -> Void

    @Environment(SyncActivityStore.self)
    private var store

    var body: some View {
        let activity = store.activity
        let showsTransfers = activity.map(Self.showsTransfers) ?? false
        if problems.needsAttention {
            Section(localizedCoreString("core.activity.attention")) {
                ForEach(problems.shown, id: \.self) { id in
                    problem(id)
                        .listRowInsets(Self.rowInsets)
                }
                if let activity, !activity.attention.isEmpty {
                    PendingRecordsView(
                        records: activity.attention,
                        perform: nil
                    )
                    .listRowInsets(Self.rowInsets)
                    Button(
                        localizedCoreString("core.activity.open_settings"),
                        action: onOpenSettings
                    )
                    .listRowInsets(Self.rowInsets)
                }
            }
        }
        if let activity, showsTransfers {
            Section {
                TransferRows(activity: activity)
                    .listRowInsets(Self.rowInsets)
            }
        }
        if !problems.needsAttention, !showsTransfers {
            Section {
                Text(localizedCoreString("core.activity.idle"))
                    .foregroundStyle(.secondary)
                    .listRowInsets(Self.rowInsets)
            }
        }
    }

    /// The board's row inset: every row of a box starts and ends this far in.
    private static let rowInsets = EdgeInsets(
        top: 0,
        leading: ThemeSpace.group,
        bottom: 0,
        trailing: ThemeSpace.group
    )

    private static func showsTransfers(_ activity: BridgeSyncActivity) -> Bool {
        activity.syncing || activity.uploads != nil || activity.pins != nil
            || activity.makingLocal != nil
    }

    @ViewBuilder
    private func problem(_ id: ActivityProblemID) -> some View {
        let dismiss = { _ = dismissed.insert(id) }
        switch id {
        case .signInSettings, .libraryLoad:
            if let problem = app.first(where: { $0.id == id }) {
                Notice(
                    title: problem.title,
                    advice: problem.advice,
                    error: problem.error,
                    look: .quiet,
                    detailsOpening: .row,
                    onDismiss: dismiss
                )
            }
        case .syncStatusUnread:
            if let error = store.error {
                Notice(
                    title: String(localized: "Couldn't read sync status"),
                    advice: nil,
                    error: error,
                    look: .quiet,
                    detailsOpening: .row,
                    onDismiss: dismiss
                )
            }
        case .syncFailure:
            if let failure = store.activity?.failure,
                let displayed = DisplayError(failure)
            {
                Notice(
                    error: displayed,
                    tone: .warning,
                    look: .quiet,
                    action: ErrorAction(
                        verbatim: localizedCoreString(
                            "core.activity.open_settings"
                        ),
                        perform: onOpenSettings
                    ),
                    buttons: .below,
                    detailsOpening: .row,
                    onDismiss: dismiss
                )
            }
        }
    }
}

/// A pass running, then the Uploading, Pinning and Making local lines, each
/// while its batch holds a release, counting its releases done or showing
/// what holds its queue back in the count's place.
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

/// One transfer line: its words, and at the end how many of its releases are
/// done, or what holds its queue back.
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
