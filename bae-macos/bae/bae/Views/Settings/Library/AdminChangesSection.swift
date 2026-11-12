import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("AdminChangesSection")

/// Where an admin change shows: next to what it changes. A rename shows under
/// the library's name, a role change on that person's page, and the rest on
/// People and devices.
enum AdminChangePlace: Equatable {
    case library
    case person(member: String)
    case people
}

extension BridgeAdminChange {
    var place: AdminChangePlace {
        switch change {
        case .renameLibrary: .library
        case .setRole(let member, _, _): .person(member: member)
        case .removePerson, .removeDevice, .approve: .people
        }
    }
}

extension MembershipStore {
    /// The admin changes this Mac asked for that show in `place`.
    func adminChanges(at place: AdminChangePlace) -> [BridgeAdminChange] {
        adminChanges.filter { $0.place == place }
    }
}

/// The admin changes this Mac asked for that show in one place: renaming the
/// library, changing a role, removing a person or a device. Each shows
/// waiting until sync publishes it, then done, refused with why, or stopped
/// with why this Mac stopped syncing for good before it was made, until the
/// person dismisses it. Shown only while there is one.
struct AdminChangesSection: View {
    @Environment(MembershipStore.self)
    private var store
    let place: AdminChangePlace
    @Environment(Sync.self)
    private var sync

    @State
    private var task: Task<Void, Never>?
    @State
    private var error: DisplayError?

    var body: some View {
        let changes = store.adminChanges(at: place)
        if !changes.isEmpty {
            Section("Changes from this Mac") {
                ForEach(changes, id: \.operation) { change in
                    AdminChangeRow(
                        change: change,
                        busy: task != nil,
                        onDismiss: { dismiss(change) }
                    )
                }
                if let error {
                    Notice(error: error)
                }
            }
        }
    }

    private func dismiss(_ change: BridgeAdminChange) {
        error = nil
        task = Task {
            defer { task = nil }
            do { try await sync.acknowledgeAdminChange(change.operation) }
            catch is CancellationError {
                logger.debug("dismissing an admin change cancelled")
            }
            catch {
                logger.error(
                    "dismissing an admin change failed: \(String(reflecting: error))"
                )
                self.error = DisplayError(error)
            }
        }
    }
}

/// One change: what it asked for, how it stands, and Dismiss once it has
/// finished.
private struct AdminChangeRow: View {
    let change: BridgeAdminChange
    let busy: Bool
    let onDismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.line) {
            HStack(spacing: ThemeSpace.related) {
                Text(description)
                    .themeText(.body)
                Spacer()
                switch change.state {
                case .waiting:
                    StatusChip("Waiting to sync")
                case .done:
                    StatusChip("Done", tone: .success)
                    dismissButton
                case .refused:
                    StatusChip("Refused", tone: .danger)
                    dismissButton
                case .stopped:
                    StatusChip("Stopped", tone: .danger)
                    dismissButton
                }
            }
            if let why {
                Text(why)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, ThemeSpace.line)
    }

    /// Why a change ended without being made.
    private var why: String? {
        switch change.state {
        case .waiting, .done: nil
        case .refused(let reason): reason.label
        case .stopped(let stop): stop.label
        }
    }

    private var dismissButton: some View {
        Button("Dismiss", action: onDismiss)
            .disabled(busy)
    }

    private var description: String {
        switch change.change {
        case .renameLibrary(let name):
            String(localized: "Rename the library to \(name)")
        case .setRole(_, let person, let role):
            switch role {
            case .admin:
                String(localized: "Make \(personName(person)) an admin")
            case .member:
                String(localized: "Make \(personName(person)) a member")
            }
        case .removePerson(_, let person):
            String(localized: "Remove \(personName(person))")
        case .removeDevice(_, let name):
            // A device this Mac hasn't read yet has no name to show.
            if let name {
                String(localized: "Remove \(name)")
            }
            else {
                String(localized: "Remove a device")
            }
        case .approve(_, let person):
            String(localized: "Let \(personName(person)) in")
        }
    }

    private func personName(_ person: String?) -> String {
        person ?? String(localized: "This person")
    }
}

#if DEBUG
    #Preview("Changes") {
        Form {
            AdminChangesSection(place: .library)
            AdminChangesSection(place: .people)
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 300)
        .environment(
            PreviewData.previewSync(storage: PreviewData.previewStorage)
        )
        .environment(
            MembershipStore.stub(
                people: PreviewData.members(
                    reaching: PreviewData.previewStorage
                ),
                selfIsAdmin: true,
                adminChanges: [
                    BridgeAdminChange(
                        operation: 1,
                        change: .renameLibrary(name: "Home"),
                        state: .waiting
                    ),
                    BridgeAdminChange(
                        operation: 2,
                        change: .removePerson(member: "member", person: "Ben"),
                        state: .refused(reason: .noAdminLeft)
                    ),
                ]
            )
        )
    }
#endif
