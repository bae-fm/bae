import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MembersSection")

/// The library's devices, shown only while sync is connected; an owner can add
/// a device or remove one, which rotates the library key.
struct MembersSection: View {
    @Environment(Sync.self)
    var sync

    @State
    private var membership: BridgeMembership?
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

    var body: some View {
        Section("Devices") {
            switch membership {
            case nil:
                if let loadError {
                    Text(loadError)
                        .foregroundStyle(Theme.danger)
                        .themeText(.body)
                }
                else {
                    ProgressView()
                        .frame(maxWidth: .infinity)
                }
            case .some(let membership):
                ForEach(membership.members, id: \.pubkey) { member in
                    MemberRow(
                        member: member,
                        onRemove: { removeConfirm = member },
                    )
                }
            }
        }
        .task { load() }
        .onDisappear {
            loadTask?.cancel()
            removeTask?.cancel()
        }
        .sheet(isPresented: $showApprove) {
            ApproveDeviceSheet(
                sync: sync,
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

        if let actionError {
            Text(actionError)
                .foregroundStyle(Theme.danger)
                .themeText(.body)
        }

        if membership?.selfIsOwner == true {
            Section {
                Button("Add a device...") {
                    actionError = nil
                    showApprove = true
                }
            }
        }
    }

    // MARK: - Actions

    private func load() {
        loadError = nil
        loadTask?.cancel()
        loadTask = Task { @MainActor in
            do {
                membership = try await sync.getMembers()
            }
            catch is CancellationError {
                logger.debug("member list load cancelled")
            }
            catch {
                logger.error(
                    "Failed to load members: \(error.localizedDescription)"
                )
                // Keep a loaded list visible; show the inline error only
                // when there is none.
                if membership == nil {
                    loadError = error.displayLine
                }
                else {
                    actionError = error.displayLine
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
                actionError = error.displayLine
            }
        }
    }
}

/// One device: its key fingerprint, role, a "This device" marker, and a remove
/// button when it can be removed.
private struct MemberRow: View {
    let member: BridgeMember
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(member.fingerprint)
                    .themeText(.mono)
                // Hidden rather than removed so every row has the same height.
                Text("This device")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .opacity(member.isSelf ? 1 : 0)
            }
            Spacer()
            RoleBadge(role: member.role)
            // Hidden rather than removed so rows keep the same layout.
            Button(role: .destructive) {
                onRemove()
            } label: {
                Image(systemName: "trash")
                    .font(.callout)
            }
            .buttonStyle(.borderless)
            .opacity(member.canRemove ? 1 : 0)
            .allowsHitTesting(member.canRemove)
        }
    }
}

private struct RoleBadge: View {
    let role: BridgeMemberRole

    var body: some View {
        Text(label)
            .themeText(.chip)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(Color.secondary.opacity(ThemeOpacity.tint))
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

#if DEBUG
    #Preview("Devices") {
        Form {
            MembersSection()
        }
        .formStyle(.grouped)
        .frame(width: 500, height: 320)
        .environment(PreviewData.previewSync())
    }
#endif
