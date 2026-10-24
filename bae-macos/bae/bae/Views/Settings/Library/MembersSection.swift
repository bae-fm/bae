import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MembersSection")

/// The library's people and their devices, and the invites this device has
/// open. Admins invite people, change roles and remove people; each person
/// adds and removes their own devices, and an admin can remove anyone's. A
/// sent invite's request opens a sheet to approve or decline it.
struct MembersSection: View {
    @Environment(MembershipStore.self)
    private var store
    @Environment(Sync.self)
    private var sync
    @Environment(SyncActivityStore.self)
    private var syncActivity

    /// The invite sheet showing: the form, or an open invite's code.
    private enum InviteSheet: Identifiable {
        case form
        case code(inviteId: String)

        var id: String {
            switch self {
            case .form: ""
            case .code(let inviteId): inviteId
            }
        }
    }

    @State
    private var inviteSheet: InviteSheet?
    /// Sent invites whose request sheet the person closed; the invite's row
    /// opens it again.
    @State
    private var closedRequests: Set<String> = []
    @State
    private var showAddDevice = false
    /// The QR code and recovery code the Add Device sheet shows; nil while
    /// they generate.
    @State
    private var addDeviceShare: Result<BridgeRestoreCodeShare, Error>?
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
        Section("People") {
            if let members = store.members {
                ForEach(members, id: \.id) { person in
                    if let member = person.membership {
                        MemberRow(
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
                            onAddDevice: {
                                addDeviceShare = nil
                                showAddDevice = true
                            }
                        )
                    }
                }
            }
            else if store.error == nil {
                ProgressView()
                    .frame(maxWidth: .infinity)
            }
            if let error = store.error {
                ErrorDetailDisclosure(error: error)
            }
            if let actionError {
                ErrorDetailDisclosure(error: actionError)
            }
            ForEach(store.invites, id: \.id) { invite in
                InviteRow(
                    invite: invite,
                    onOpen: { open(invite) },
                    onRefresh: { Task { await syncActivity.retryNow() } }
                )
            }
            if store.selfIsAdmin {
                Button("Invite a Person...") { inviteSheet = .form }
            }
        }
        .sheet(item: $inviteSheet) { sheet in
            switch sheet {
            case .form:
                InvitePersonSheet(
                    onCreated: { inviteSheet = .code(inviteId: $0) },
                    onDismiss: { inviteSheet = nil }
                )
            case .code(let inviteId):
                InviteCodeSheet(
                    inviteId: inviteId,
                    onDismiss: { inviteSheet = nil }
                )
            }
        }
        .sheet(
            isPresented: Binding(
                get: { requestShown != nil },
                set: { shown in
                    if !shown, let invite = requestShown {
                        closedRequests.insert(invite.id)
                    }
                }
            )
        ) {
            if let invite = requestShown {
                JoinRequestSheet(
                    invite: invite,
                    busy: task != nil,
                    error: actionError,
                    onApprove: { approve(invite) },
                    onDecline: { decline(invite) },
                    onClose: { closedRequests.insert(invite.id) }
                )
            }
        }
        .sheet(isPresented: $showAddDevice) {
            AddDeviceSheet(
                result: $addDeviceShare,
                onDismiss: { showAddDevice = false }
            )
            .task { await generateAddDeviceShare() }
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
            case .person(let member, _):
                if let forgetLine = member.access.forgetLine {
                    Text(
                        "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                    ) + Text(verbatim: "\n\n\(forgetLine)")
                }
                else {
                    Text(
                        "They lose access to the library. Their private history and private playlists are deleted; what they shared stays."
                    )
                }
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

    /// The first sent invite whose request waits for a decision, unless the
    /// person closed its sheet.
    private var requestShown: BridgeInvite? {
        store.invites.first {
            $0.requestToDecide != nil && !closedRequests.contains($0.id)
        }
    }

    /// Open an invite from its row: its request to decide, or its code.
    private func open(_ invite: BridgeInvite) {
        if invite.requestToDecide != nil {
            closedRequests.remove(invite.id)
        }
        else {
            inviteSheet = .code(inviteId: invite.id)
        }
    }

    private func approve(_ invite: BridgeInvite) {
        guard let request = invite.requestToDecide else { return }
        run { try await sync.approveJoinRequest(request) }
    }

    private func decline(_ invite: BridgeInvite) {
        guard let request = invite.requestToDecide else { return }
        run { try await sync.declineJoinRequest(request) }
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

    /// Closing the sheet cancels this task, and with it the Rust call.
    private func generateAddDeviceShare() async {
        do {
            addDeviceShare = .success(try await sync.generateRestoreCode())
        }
        catch is CancellationError {
            logger.debug("add-device code generation cancelled")
        }
        catch {
            logger.error(
                "Failed to generate the add-device code: \(error.localizedDescription)"
            )
            // No line means core reported a cancellation; keep the spinner.
            guard DisplayError(error) != nil else { return }
            addDeviceShare = .failure(error)
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
/// with Add Device under this device's own person and their removed devices
/// collapsed below.
private struct MemberRow: View {
    let name: String?
    let member: BridgeMember
    let canChangeRole: Bool
    let busy: Bool
    let onSetRole: (BridgeMemberRole) -> Void
    let onRemove: () -> Void
    let onRemoveDevice: (BridgeActiveDevice) -> Void
    let onAddDevice: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: ThemeSpace.related) {
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
                .menuStyle(.borderlessButton)
                .fixedSize()
                .disabled(!canChangeRole || busy)
                Button(role: .destructive, action: onRemove) {
                    Image(systemName: "person.badge.minus")
                        .themeIcon(.medium)
                }
                .buttonStyle(.borderless)
                .help("Remove from the library")
                .opacity(member.canRemove ? 1 : 0)
                .allowsHitTesting(member.canRemove && !busy)
                .accessibilityHidden(!member.canRemove)
            }
            // Every member of a library reaches it the same way, so every row
            // has this line, or none does.
            if let keyLine = member.access.keyLine {
                Text(verbatim: keyLine)
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            ForEach(member.activeDevices, id: \.id) { device in
                DeviceRow(
                    device: device,
                    busy: busy,
                    onRemove: { onRemoveDevice(device) }
                )
            }
            // A row's person never changes, so including this only in one's
            // own row never changes a row's size after it first lays out.
            if member.isSelf {
                Button("Add Device...", action: onAddDevice)
                    .buttonStyle(.link)
                    .padding(.leading, ThemeSpace.group)
            }
            // Like the device rows above, this changes only when the person's
            // devices do: their first device loses access.
            if !member.formerDevices.isEmpty {
                FormerDevicesDisclosure(devices: member.formerDevices)
            }
        }
        .padding(.vertical, ThemeSpace.line)
    }
}

/// A device that can sync, removable by this device's person when allowed.
private struct DeviceRow: View {
    let device: BridgeActiveDevice
    let busy: Bool
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Image(systemName: "laptopcomputer.and.iphone")
                .themeIcon(.small)
                .foregroundStyle(.secondary)
            Text(device.name)
                .themeText(.body)
            // Hidden rather than removed so every row has the same layout.
            Text("This Mac")
                .themeText(.detail)
                .foregroundStyle(.secondary)
                .opacity(device.isSelf ? 1 : 0)
                .accessibilityHidden(!device.isSelf)
            Spacer()
            Button(role: .destructive, action: onRemove) {
                Image(systemName: "minus.circle")
                    .themeIcon(.small)
            }
            .buttonStyle(.borderless)
            .help("Remove this device")
            .opacity(device.canRemove ? 1 : 0)
            .allowsHitTesting(device.canRemove && !busy)
            .accessibilityHidden(!device.canRemove)
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
                    Text(device.state.label)
                        .themeText(.detail)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            }
        } label: {
            Text("Removed devices (\(devices.count))")
                .themeText(.detail)
                .foregroundStyle(.secondary)
        }
        .padding(.leading, ThemeSpace.group)
    }
}

/// One of this device's open invites: the person's name, marked invited,
/// when it expires, and a refresh that checks for its request now.
private struct InviteRow: View {
    let invite: BridgeInvite
    let onOpen: () -> Void
    let onRefresh: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            Button(action: onOpen) {
                HStack(spacing: ThemeSpace.related) {
                    Text(verbatim: invite.invitee ?? "")
                        .themeText(.rowTitle)
                    StatusChip("Invited")
                    Text(
                        "Expires \(invite.expiresAt, format: .relative(presentation: .named))"
                    )
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    Spacer()
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            Button(action: onRefresh) {
                Image(systemName: "arrow.clockwise")
                    .themeIcon(.small)
            }
            .buttonStyle(.borderless)
            .help("Check for Requests")
        }
        .padding(.vertical, ThemeSpace.line)
    }
}

/// A sent invite's request: the person's name and the number their device
/// shows, to compare before approving. Closing it keeps the request; the
/// invite's row opens it again.
private struct JoinRequestSheet: View {
    let invite: BridgeInvite
    let busy: Bool
    let error: DisplayError?
    let onApprove: () -> Void
    let onDecline: () -> Void
    let onClose: () -> Void

    var body: some View {
        VStack(spacing: ThemeSpace.group) {
            Text(verbatim: invite.invitee ?? "")
                .themeText(.heading)
            if let info = invite.requestToDecide?.info() {
                Text("Confirmation number")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                Text(verbatim: info.confirmation)
                    .themeText(.title)
                    .textSelection(.enabled)
            }
            if let error {
                ErrorDetailDisclosure(error: error)
            }
            HStack(spacing: ThemeSpace.group) {
                Button("Close", action: onClose)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Decline", role: .destructive, action: onDecline)
                    .disabled(busy)
                Button("Approve", action: onApprove)
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(busy)
            }
        }
        .padding()
        .frame(width: 320)
    }
}

#if DEBUG
    #Preview("People") {
        Form {
            MembersSection()
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 360)
        .environment(PreviewData.previewSync())
        .environment(SyncActivityStore.preview(needsAttention: false))
        .environment(PreviewData.configStore())
        .environment(
            MembershipStore.stub(
                people: PreviewData.members,
                selfIsAdmin: true
            )
        )
    }
#endif
