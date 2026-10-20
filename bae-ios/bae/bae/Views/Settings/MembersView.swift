import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MembersView")

/// The library's people and their devices. Admins invite people, approve or
/// decline their join requests, change roles and remove people; each person
/// adds and removes their own devices, and an admin can remove anyone's.
struct MembersView: View {
    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync

    @State
    private var showInvite = false
    @State
    private var showAddDevice = false
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
        case device(BridgeActiveDevice)
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
                            onRemoveDevice: { removal = .device($0) },
                            onAddDevice: { showAddDevice = true }
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
        .sheet(isPresented: $showAddDevice) {
            AddDeviceView(
                generate: sync.generateRestoreCode,
                onDismiss: { showAddDevice = false }
            )
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

/// One member: name, role, the "You" marker, and each of their active devices,
/// each removable by swiping when this person may remove it, with Add Device
/// under this device's own person and their removed devices collapsed below.
private struct PersonSection: View {
    let name: String?
    let member: BridgeMember
    let canChangeRole: Bool
    let busy: Bool
    let onSetRole: (BridgeMemberRole) -> Void
    let onRemove: () -> Void
    let onRemoveDevice: (BridgeActiveDevice) -> Void
    let onAddDevice: () -> Void

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
            ForEach(member.activeDevices, id: \.id) { device in
                DeviceRow(device: device)
                    .swipeActions {
                        if device.canRemove {
                            Button("Remove", role: .destructive) {
                                onRemoveDevice(device)
                            }
                            .disabled(busy)
                        }
                    }
            }
            // A section's person never changes, so including this only in
            // one's own section never changes a section's size after it first
            // lays out.
            if member.isSelf {
                Button("Add Device\u{2026}", action: onAddDevice)
            }
            // Like the device rows above, this changes only when the person's
            // devices do: their first device loses access.
            if !member.formerDevices.isEmpty {
                FormerDevicesDisclosure(devices: member.formerDevices)
            }
        }
    }
}

private struct DeviceRow: View {
    let device: BridgeActiveDevice

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "laptopcomputer.and.iphone")
                .themeIcon(.small)
                .foregroundStyle(.secondary)
            Text(device.name)
                .themeText(.body)
            Spacer()
            // Hidden rather than removed so every row has the same layout.
            Text("This device")
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .opacity(device.isSelf ? 1 : 0)
                .accessibilityHidden(!device.isSelf)
        }
        .padding(.leading, ThemeSpace.group)
    }
}

/// A person's removed and replaced devices, collapsed until opened. They no
/// longer have access, so they offer no actions.
private struct FormerDevicesDisclosure: View {
    let devices: [BridgeFormerDevice]

    var body: some View {
        DisclosureGroup {
            ForEach(devices, id: \.id) { device in
                HStack(spacing: ThemeSpace.related) {
                    Image(systemName: "laptopcomputer.and.iphone")
                        .themeIcon(.small)
                        .foregroundStyle(.secondary)
                    Text(device.name)
                        .themeText(.body)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Text(device.state.label)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                }
                .padding(.leading, ThemeSpace.group)
            }
        } label: {
            Text("Removed devices (\(devices.count))")
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .padding(.leading, ThemeSpace.group)
        }
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
    .environment(
        MembershipStore.stub(people: PreviewData.members, selfIsAdmin: true)
    )
    .previewStores()
}
#endif
