import BaeKit
import SwiftUI
import os.log

private let logger = Logger.bae("MembersView")

/// The library's member devices; an owner can add a device or remove one,
/// which rotates the library key.
struct MembersView: View {
    @Environment(Sync.self)
    private var sync

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
        List {
            Section("Devices") {
                switch membership {
                case nil:
                    if let loadError {
                        VStack(alignment: .leading, spacing: ThemeSpace.related) {
                            ErrorText(loadError)
                            Button("Retry") { load() }
                        }
                    }
                    else {
                        ProgressView()
                            .frame(maxWidth: .infinity)
                    }
                case .some(let membership):
                    ForEach(membership.members, id: \.pubkey) { member in
                        MemberRow(
                            member: member,
                            onRemove: { removeConfirm = member }
                        )
                    }
                }
            }

            if let actionError {
                Section {
                    ErrorText(actionError)
                }
            }

            if membership?.selfIsOwner == true {
                Section {
                    Button("Add a device\u{2026}") {
                        actionError = nil
                        showApprove = true
                    }
                }
            }
        }
        .navigationTitle("Members")
        .navigationBarTitleDisplayMode(.inline)
        .task { load() }
        .onDisappear {
            loadTask?.cancel()
            removeTask?.cancel()
        }
        .sheet(isPresented: $showApprove) {
            ApproveDeviceView(
                sync: sync,
                onDismiss: { showApprove = false },
                onApproved: { load() }
            )
        }
        .confirmationDialog(
            "Remove this device?",
            isPresented: Binding(
                get: { removeConfirm != nil },
                set: { if !$0 { removeConfirm = nil } }
            ),
            titleVisibility: .visible
        ) {
            Button("Remove", role: .destructive) {
                if let member = removeConfirm {
                    remove(member)
                }
                removeConfirm = nil
            }
            Button("Cancel", role: .cancel) {}
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
                membership = try await sync.getMembers()
            }
            catch is CancellationError {
                logger.debug("member list load cancelled")
            }
            catch {
                logger.error(
                    "Failed to load members: \(error.localizedDescription)"
                )
                // Keep a loaded list visible; show the inline error only when
                // there is none.
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

/// One device row: key fingerprint, role, a "This device" marker, and Remove
/// when the member can be removed.
private struct MemberRow: View {
    let member: BridgeMember
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: ThemeSpace.related) {
            VStack(alignment: .leading, spacing: ThemeSpace.line) {
                Text(member.fingerprint)
                    .themeText(.mono)
                // Hidden, not removed, so every row has the same height.
                Text("This device")
                    .themeText(.detail)
                    .foregroundStyle(.secondary)
                    .opacity(member.isSelf ? 1 : 0)
            }
            Spacer()
            RoleChip(role: member.role)
            // Hidden, not removed, so every row keeps the same layout.
            Button(role: .destructive) {
                onRemove()
            } label: {
                Image(systemName: "trash")
                    .themeIcon(.medium)
            }
            .buttonStyle(.borderless)
            .opacity(member.canRemove ? 1 : 0)
            .allowsHitTesting(member.canRemove)
        }
    }
}

private struct RoleChip: View {
    let role: BridgeMemberRole

    var body: some View {
        StatusChip(label)
    }

    private var label: LocalizedStringKey {
        switch role {
        case .owner:
            "Owner"
        case .member:
            "Member"
        case .follower:
            "Follower"
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
