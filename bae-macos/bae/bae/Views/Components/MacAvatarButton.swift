import BaeKit
import SwiftUI

/// The person's picture at the end of the title bar, which opens their menu
/// (`PersonMenuItems`). A library with no cloud home yet starts the menu with
/// Set up a cloud home…, and Add device and Invite someone each open a sheet
/// that says the cloud home comes first. The sheets the menu opens are the
/// window's. While the person has no profile, a plain tile stands in the
/// picture, so the menu stays reachable. The button draws nothing until the
/// sync state has been read, because the menu's first items come from it.
struct MacAvatarButton: View {
    @Environment(MembershipStore.self)
    private var membership
    @Environment(SyncStatusStore.self)
    private var syncStatus
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings

    @State
    private var sheet: PersonMenuSheet?

    var body: some View {
        if let hasCloudHome = syncStatus.snapshot?.hasCloudHome {
            MacButton(kind: .plain, menu: { items(hasCloudHome: hasCloudHome) })
            {
                picture
                    .frame(
                        width: ThemeSize.hitTarget,
                        height: ThemeSize.hitTarget
                    )
                    .contentShape(Rectangle())
            }
            .help("Your menu")
            .accessibilityLabel("Your menu")
            .sheet(item: $sheet) { shown in
                sheetContent(shown)
            }
        }
    }

    /// The person's picture, or a plain tile while they have no profile.
    @ViewBuilder
    private var picture: some View {
        if let profile = membership.selfProfile {
            Avatar(profile: profile, size: 22)
        }
        else {
            Avatar(initials: "", size: 22)
        }
    }

    @ViewBuilder
    private func sheetContent(_ shown: PersonMenuSheet) -> some View {
        switch shown {
        case .addDevice:
            AddDevicePairingSheet(onDismiss: { sheet = nil })
        case .invite:
            InvitePersonSheet(
                onCreated: { sheet = .inviteCode($0) },
                onDismiss: { sheet = nil }
            )
        case .inviteCode(let inviteId):
            InviteCodeSheet(inviteId: inviteId, onDismiss: { sheet = nil })
        case .cloudHomeFirst(let reason):
            CloudHomeFirstSheet(
                reason: reason,
                onCancel: { sheet = nil },
                onSetUp: { sheet = .cloudHomeSetup }
            )
        case .cloudHomeSetup:
            SyncSetupWizard(onDone: { sheet = nil })
        }
    }

    private func items(hasCloudHome: Bool) -> [MacMenuEntry] {
        PersonMenuItems(
            hasCloudHome: hasCloudHome,
            isAdmin: membership.selfIsAdmin,
            onSetUpCloudHome: { sheet = .cloudHomeSetup },
            onAddDevice: {
                sheet = hasCloudHome ? .addDevice : .cloudHomeFirst(.addDevice)
            },
            onInvite: {
                sheet = hasCloudHome ? .invite : .cloudHomeFirst(.invite)
            },
            onPlaylists: { uiStore.openSidePane(.playlists) },
            onHistory: { uiStore.openSidePane(.history) },
            onSettings: { openSettings() }
        )
        .entries
    }
}

/// The sheet the person's menu has up.
private enum PersonMenuSheet: Identifiable {
    case addDevice
    case invite
    /// An open invite's code, once the invite form created it.
    case inviteCode(String)
    /// Add device or Invite someone on a library with no cloud home yet.
    case cloudHomeFirst(CloudHomeFirstSheet.Reason)
    case cloudHomeSetup

    var id: String {
        switch self {
        case .addDevice: "add-device"
        case .invite: "invite"
        case .inviteCode(let inviteId): "invite-code-\(inviteId)"
        case .cloudHomeFirst(let reason): "cloud-home-first-\(reason)"
        case .cloudHomeSetup: "cloud-home-setup"
        }
    }
}
