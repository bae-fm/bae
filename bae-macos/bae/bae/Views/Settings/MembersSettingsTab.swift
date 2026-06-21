import SwiftUI
import os.log

private let logger = Logger.bae("MembersSettings")

/// The Members settings tab: lists the devices in this library's membership
/// chain, lets an owner approve a new device (which hands back an invite code),
/// and lets an owner remove a device (which rotates the library key). The list
/// loads when the tab appears and after each approve/remove so the chain stays
/// current. Reading the chain and the mutations all run off the main thread.
struct MembersSettingsTab: View {
    @Environment(Sync.self)
    var sync

    @State
    private var members: [BridgeMember]?
    @State
    private var loadError: String?
    @State
    private var actionError: String?
    @State
    private var loadTask: Task<Void, Never>?
    @State
    private var removeTask: Task<Void, Never>?
    @State
    private var removeConfirm: BridgeMember?
    @State
    private var showApprove = false

    /// Whether this device may approve and remove other devices. Only an owner
    /// can; a member device shows the list read-only.
    private var selfIsOwner: Bool {
        members?.contains { $0.isSelf && $0.role == .owner } ?? false
    }

    var body: some View {
        Form {
            Section("Devices") {
                switch members {
                case nil:
                    if let loadError {
                        Text(loadError)
                            .foregroundStyle(.red)
                            .font(.callout)
                    }
                    else {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                    }
                case .some(let list):
                    ForEach(list, id: \.pubkey) { member in
                        MemberRow(
                            member: member,
                            canRemove: selfIsOwner && !member.isSelf,
                            onRemove: { removeConfirm = member },
                        )
                    }
                }
            }

            if let actionError {
                Text(actionError)
                    .foregroundStyle(.red)
                    .font(.callout)
            }

            if selfIsOwner {
                Section {
                    Button("Add a device...") {
                        actionError = nil
                        showApprove = true
                    }
                }
            }
        }
        .formStyle(.grouped)
        .task { load() }
        .onDisappear {
            loadTask?.cancel()
            removeTask?.cancel()
        }
        .sheet(isPresented: $showApprove) {
            ApproveDeviceSheet(
                invite: sync.inviteMember,
                onDismiss: { showApprove = false },
                onApproved: { load() },
            )
        }
        .confirmationDialog(
            "Remove this device?",
            isPresented: Binding(
                get: { removeConfirm != nil },
                set: { if !$0 { removeConfirm = nil } },
            ),
            titleVisibility: .visible,
        ) {
            Button("Remove", role: .destructive) {
                if let member = removeConfirm {
                    remove(member)
                }
                removeConfirm = nil
            }
        } message: {
            Text(
                "It will lose access and the library key will be rotated. Devices still in the library re-key automatically."
            )
        }
    }

    // MARK: - Actions

    private func load() {
        loadError = nil
        loadTask?.cancel()
        loadTask = Task { @MainActor in
            do {
                let loaded = try await sync.getMembers()
                members = loaded
            }
            catch is CancellationError {
                logger.debug("member list load cancelled")
            }
            catch {
                logger.error(
                    "Failed to load members: \(error.localizedDescription)"
                )
                // Keep a previously loaded list visible if there is one; only
                // fall back to the inline error when there's nothing to show.
                if members == nil {
                    loadError = error.localizedDescription
                }
                else {
                    actionError = error.localizedDescription
                }
            }
        }
    }

    private func remove(_ member: BridgeMember) {
        actionError = nil
        removeTask?.cancel()
        removeTask = Task { @MainActor in
            do {
                try await sync.removeMember(member.pubkey)
                load()
            }
            catch is CancellationError {
                logger.debug("member removal cancelled")
            }
            catch {
                logger.error(
                    "Failed to remove member: \(error.localizedDescription)"
                )
                actionError = error.localizedDescription
            }
        }
    }
}

/// One device row: a short fingerprint of the public key, a role badge, and —
/// for the running device — a "This device" marker. Owners see a Remove control
/// on every other device.
private struct MemberRow: View {
    let member: BridgeMember
    let canRemove: Bool
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(MemberFormat.fingerprint(member.pubkey))
                    .font(.system(.body, design: .monospaced))
                // Always in the layout (hidden when not self) so the row height
                // doesn't change between self and other rows.
                Text("This device")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .opacity(member.isSelf ? 1 : 0)
            }
            Spacer()
            RoleBadge(role: member.role)
            // Kept in the layout always (hidden, not removed) so toggling it
            // doesn't re-measure sibling rows.
            Button(role: .destructive) {
                onRemove()
            } label: {
                Image(systemName: "trash")
                    .font(.callout)
            }
            .buttonStyle(.borderless)
            .opacity(canRemove ? 1 : 0)
            .allowsHitTesting(canRemove)
        }
    }
}

private struct RoleBadge: View {
    let role: BridgeMemberRole

    var body: some View {
        Text(label)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(Color.secondary.opacity(0.15))
            .clipShape(Capsule())
            .foregroundStyle(.secondary)
    }

    private var label: String {
        switch role {
        case .owner:
            String(localized: "Owner")
        case .member:
            String(localized: "Member")
        case .follower:
            String(localized: "Follower")
        }
    }
}
