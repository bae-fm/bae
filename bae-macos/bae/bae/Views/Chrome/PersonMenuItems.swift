import BaeKit

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
        MacMenuItem.divider
        MacMenuItem("Playlists", action: onPlaylists)
        MacMenuItem("History", action: onHistory)
        MacMenuItem.divider
        MacMenuItem("Settings…", action: onSettings)
    }
}
