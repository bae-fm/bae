import BaeKit
import SwiftUI

/// The person's picture at the end of the title bar, which opens their menu:
/// Add device… and, for an admin, Invite someone…, then Playlists and
/// History, then Settings…. A library with no cloud home yet starts the menu
/// with Set up a cloud home…, and Add device and Invite someone each open a
/// sheet that says the cloud home comes first. The sheets the menu opens are
/// the window's.
struct PersonMenu: View {
    @Environment(MembershipStore.self)
    private var membership
    @Environment(ConfigStore.self)
    private var configStore
    @Environment(SyncStatusStore.self)
    private var syncStatus
    @Environment(UiStore.self)
    private var uiStore
    @Environment(\.openSettingsWindow)
    private var openSettings

    @State
    private var sheet: PersonMenuSheet?

    var body: some View {
        MacMenuButton(content: items) {
            picture
                .frame(width: ThemeSize.hitTarget, height: ThemeSize.hitTarget)
                .contentShape(Rectangle())
        }
        .help("Your menu")
        .accessibilityLabel("Your menu")
        .sheet(item: $sheet) { shown in
            switch shown {
            case .addDevice:
                AddDevicePairingSheet(onDismiss: { sheet = nil })
            case .invite:
                InvitePersonSheet(
                    onCreated: { sheet = .inviteCode($0) },
                    onDismiss: { sheet = nil }
                )
            case .inviteCode(let inviteId):
                InviteCodeSheet(
                    inviteId: inviteId,
                    onDismiss: { sheet = nil }
                )
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
    }

    /// The person's own picture, or their initials while they have none.
    @ViewBuilder
    private var picture: some View {
        if let profile = membership.selfProfile {
            Avatar(profile: profile, size: 22)
        }
        else {
            Avatar(initials: "", size: 22)
        }
    }

    /// Whether the library has a cloud home, or a setup of one runs.
    private var hasCloudHome: Bool {
        configStore.syncs || syncStatus.snapshot?.setupRunning == true
    }

    private func items() -> [MacMenuEntry] {
        let hasCloudHome = hasCloudHome
        return PersonMenuItems(
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

/// The items of the person's menu: Set up a cloud home… first while the
/// library has none, Add device… and, for an admin, Invite someone…, then
/// Playlists and History, then Settings….
struct PersonMenuItems: MacMenuPart {
    let hasCloudHome: Bool
    let isAdmin: Bool
    let onSetUpCloudHome: () -> Void
    let onAddDevice: () -> Void
    let onInvite: () -> Void
    let onPlaylists: () -> Void
    let onHistory: () -> Void
    let onSettings: () -> Void

    var entries: [MacMenuEntry] { items() }

    @MacMenuBuilder
    private func items() -> [MacMenuEntry] {
        if !hasCloudHome {
            MacMenuItem("Set up a cloud home…", action: onSetUpCloudHome)
        }
        MacMenuItem("Add device…", action: onAddDevice)
        if isAdmin {
            MacMenuItem("Invite someone…", action: onInvite)
        }
        MacMenuDivider()
        MacMenuItem("Playlists", action: onPlaylists)
        MacMenuItem("History", action: onHistory)
        MacMenuDivider()
        MacMenuItem("Settings…", action: onSettings)
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

/// The sheet that says a device or a person joins only through a cloud
/// home: Cancel, or Set up a cloud home…, which opens the setup.
struct CloudHomeFirstSheet: View {
    /// What the person asked for.
    enum Reason {
        case addDevice
        case invite
    }

    let reason: Reason
    let onCancel: () -> Void
    let onSetUp: () -> Void

    var body: some View {
        MacSheet(title, message: message, width: 420) {
            EmptyView()
        } footer: {
            MacSheetFooter {
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Set up a cloud home…", action: onSetUp)
                    .buttonStyle(PrimaryButtonStyle())
                    .keyboardShortcut(.defaultAction)
            }
        }
    }

    private var title: LocalizedStringKey {
        switch reason {
        case .addDevice: "Add a device"
        case .invite: "Invite someone"
        }
    }

    private var message: LocalizedStringKey {
        switch reason {
        case .addDevice:
            "To add a device, set up a cloud home first. Your library and music stay on this Mac until you do."
        case .invite:
            "To invite someone, set up a cloud home first. Your library and music stay on this Mac until you do."
        }
    }
}
