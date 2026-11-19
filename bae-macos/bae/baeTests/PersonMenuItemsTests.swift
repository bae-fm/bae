import BaeKit
import Testing

@testable import bae

@MainActor
@Suite("The person's menu")
struct PersonMenuItemsTests {
    /// The menu's lines, a divider as "—".
    private func lines(hasCloudHome: Bool, isAdmin: Bool) -> [String] {
        PersonMenuItems(
            hasCloudHome: hasCloudHome,
            isAdmin: isAdmin,
            onSetUpCloudHome: {},
            onAddDevice: {},
            onInvite: {},
            onPlaylists: {},
            onHistory: {},
            onSettings: {}
        )
        .entries
        .map { entry in
            switch entry {
            case .item(let item): item.title
            case .divider: "—"
            case .header(let title): title
            case .submenu(let title, _, _): title
            }
        }
    }

    @Test("an admin of a library with a cloud home can add a device and invite")
    func adminWithCloudHome() {
        #expect(
            lines(hasCloudHome: true, isAdmin: true) == [
                String(localized: "Add device…"),
                String(localized: "Invite someone…"),
                "—",
                String(localized: "Playlists"),
                String(localized: "History"),
                "—",
                String(localized: "Settings…"),
            ]
        )
    }

    @Test("a member who is no admin has no Invite someone")
    func memberHasNoInvite() {
        #expect(
            !lines(hasCloudHome: true, isAdmin: false)
                .contains(String(localized: "Invite someone…"))
        )
    }

    @Test("a library with no cloud home starts the menu with its setup")
    func noCloudHomeStartsWithSetup() {
        #expect(
            lines(hasCloudHome: false, isAdmin: true).prefix(3) == [
                String(localized: "Set up a cloud home…"),
                String(localized: "Add device…"),
                String(localized: "Invite someone…"),
            ]
        )
    }
}
