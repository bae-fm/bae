import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MembersView")

/// The library's people and their devices. Admins invite people, approve or
/// decline their join requests, change roles and remove people; each person
/// removes their own devices, and an admin can remove anyone's.
struct MembersView: View {
    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync

    @State
    private var showInvite = false
    @State
    private var removal: Removal?
    /// What the person still has to do in the provider after removing a
    /// device.
    @State
    private var signOut: BridgeProviderSignOut?
    @State
    private var task: Task<Void, Never>?
    @State
    private var actionError: DisplayError?

    private enum Removal {
        case person(BridgeMember, name: String)
        case device(BridgeDevice)
    }

    var body: some View {
        List {
            if let members = store.members {
                ForEach(members, id: \.id) { person in
                    if let member = person.membership {
                        PersonSection(
                            name: person.profile?.name,
                            member: member,
                            canChangeRole: store.selfIsAdmin,
                            busy: task != nil,
                            onSetRole: { setRole(member, $0) },
                            onRemove: {
                                removal = .person(
                                    member,
                                    name: person.profile?.name
                                        ?? String(localized: "This person")
                                )
                            },
                            onRemoveDevice: { removal = .device($0) }
                        )
                    }
                }
            }
            else if store.error == nil {
                ProgressView()
                    .frame(maxWidth: .infinity)
            }
            if store.error != nil || actionError != nil {
                Section {
                    ErrorDetailDisclosure(error: store.error)
                    ErrorDetailDisclosure(error: actionError)
                }
            }
            if store.selfIsAdmin {
                if !store.requests.isEmpty {
                    Section("Waiting to join") {
                        ForEach(store.requests, id: \.info.invite) { pending in
                            JoinRequestRow(
                                info: pending.info,
                                busy: task != nil,
                                onApprove: {
                                    run {
                                        try await sync.approveJoinRequest(
                                            pending.request
                                        )
                                    }
                                },
                                onDecline: {
                                    run {
                                        try await sync.declineJoinRequest(
                                            pending.request
                                        )
                                    }
                                }
                            )
                        }
                    }
                }
                Section {
                    Button("Invite a Person\u{2026}") { showInvite = true }
                }
            }
        }
        .navigationTitle("People")
        .navigationBarTitleDisplayMode(.inline)
        .onDisappear { task?.cancel() }
        .sheet(isPresented: $showInvite) {
            InvitePersonView(onDismiss: { showInvite = false })
        }
        .confirmationDialog(
            removalTitle,
            isPresented: Binding(
                get: { removal != nil },
                set: { if !$0 { removal = nil } }
            ),
            titleVisibility: .visible,
            presenting: removal
        ) { selected in
            Button("Remove", role: .destructive) { remove(selected) }
            Button("Cancel", role: .cancel) {}
        } message: { selected in
            switch selected {
            case .person:
                Text(
                    "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                )
            case .device:
                Text("The device loses access to the library.")
            }
        }
        .alert(
            "Device removed",
            isPresented: Binding(
                get: { signOut != nil },
                set: { if !$0 { signOut = nil } }
            ),
            presenting: signOut
        ) { _ in
            Button("OK", role: .cancel) {}
        } message: { instruction in
            Text(instruction.text)
        }
    }

    private var removalTitle: String {
        switch removal {
        case .person(_, let name): String(localized: "Remove \(name)?")
        case .device(let device): String(localized: "Remove \(device.name)?")
        case nil: ""
        }
    }

    private func setRole(_ member: BridgeMember, _ role: BridgeMemberRole) {
        run { try await sync.setMemberRole(member.id, role) }
    }

    private func remove(_ removal: Removal) {
        switch removal {
        case .person(let member, _):
            run { try await sync.removeMember(member.id) }
        case .device(let device):
            run { signOut = try await sync.removeDevice(device.id) }
        }
    }

    private func run(_ operation: @escaping () async throws -> Void) {
        actionError = nil
        task = Task {
            defer { task = nil }
            do { try await operation() }
            catch is CancellationError {
                logger.debug("membership change cancelled")
            }
            catch {
                logger.error(
                    "membership change failed: \(error.localizedDescription)"
                )
                actionError = DisplayError(error)
            }
        }
    }
}

/// One member: name, role, the "You" marker, and each of their devices, each
/// removable by swiping when this person may remove it.
private struct PersonSection: View {
    let name: String?
    let member: BridgeMember
    let canChangeRole: Bool
    let busy: Bool
    let onSetRole: (BridgeMemberRole) -> Void
    let onRemove: () -> Void
    let onRemoveDevice: (BridgeDevice) -> Void

    var body: some View {
        Section {
            HStack(spacing: ThemeSpace.related) {
                Text(name ?? String(localized: "No profile yet"))
                    .themeText(.rowTitle)
                    .foregroundStyle(name == nil ? .secondary : .primary)
                // Hidden rather than removed so every row has the same layout.
                Text("You")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .opacity(member.isSelf ? 1 : 0)
                Spacer()
                Menu {
                    Button("Make Admin") { onSetRole(.admin) }
                        .disabled(member.role == .admin)
                    Button("Make Member") { onSetRole(.member) }
                        .disabled(member.role == .member)
                } label: {
                    StatusChip(verbatim: member.role.label)
                }
                .disabled(!canChangeRole || busy)
            }
            .swipeActions {
                if member.canRemove {
                    Button("Remove", role: .destructive, action: onRemove)
                        .disabled(busy)
                }
            }
            ForEach(member.devices, id: \.id) { device in
                DeviceRow(device: device)
                    .swipeActions {
                        if member.canRemoveDevices, device.state == .active,
                            !device.isSelf
                        {
                            Button("Remove", role: .destructive) {
                                onRemoveDevice(device)
                            }
                            .disabled(busy)
                        }
                    }
            }
        }
    }
}

private struct DeviceRow: View {
    let device: BridgeDevice

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "laptopcomputer.and.iphone")
                .themeIcon(.small)
                .foregroundStyle(.secondary)
            Text(device.name)
                .themeText(.body)
            Spacer()
            Text(device.stateLabel(thisDevice: String(localized: "This device")))
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
        .padding(.leading, ThemeSpace.group)
    }
}

/// A join request an admin approves or declines: the requesting device and the
/// provider account it signed in with.
private struct JoinRequestRow: View {
    let info: BridgeJoinRequestInfo
    let busy: Bool
    let onApprove: () -> Void
    let onDecline: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
            Text(info.deviceName)
                .themeText(.rowTitle)
            Text(info.providerAccountEmail ?? "")
                .themeText(.detail)
                .foregroundStyle(.secondary)
            HStack(spacing: ThemeSpace.related) {
                Button("Decline", role: .destructive, action: onDecline)
                    .buttonStyle(.bordered)
                Button("Approve", action: onApprove)
                    .buttonStyle(PrimaryButtonStyle())
            }
            .disabled(busy)
        }
    }
}

#if DEBUG
#Preview {
    NavigationStack {
        MembersView()
    }
    .previewStores()
}
#endif
