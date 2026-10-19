import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// The main menu the app installs at launch: the standard macOS menus with
/// their standard actions and bae's commands beside them, and an Edit menu
/// whose items nothing replaces.
@MainActor
@Suite("Main menu")
struct MainMenuItemsTests {
    @Test("the installed menu has the standard menus, in order")
    func standardMenus() throws {
        let menu = try #require(NSApp.mainMenu)
        #expect(
            menu.items.map(\.title)
                == [
                    "bae", "File", "Edit", "View", "Playback", "Window", "Help",
                ]
        )
        #expect(NSApp.windowsMenu === (try MenuLookup.submenu(menu, at: 5)))
        #expect(NSApp.helpMenu === (try MenuLookup.submenu(menu, at: 6)))
    }

    /// Each menu's standard actions, and bae's commands among them.
    @Test(
        "each menu has its standard actions and bae's commands",
        arguments: [
            (
                0,
                [
                    "orderFrontStandardAboutPanel:", "checkForUpdates:",
                    "showSettingsWindow:", "hide:", "hideOtherApplications:",
                    "unhideAllApplications:", "terminate:",
                ]
            ),
            (
                1,
                [
                    "newLibrary:", "joinLibrary:", "restoreFromCode:",
                    "previousLibrary:", "nextLibrary:", "importFolder:",
                    "renameLibrary:", "syncNow:", "revealLibraryInFinder:",
                    "copyLibraryId:", "closeLibrary:", "performClose:",
                ]
            ),
            (
                2,
                [
                    "undo:", "redo:", "cut:", "copy:", "paste:",
                    "pasteAsPlainText:", "delete:", "selectAll:",
                    "performFindPanelAction:",
                    "centerSelectionInVisibleArea:", "showGuessPanel:",
                    "toggleContinuousSpellChecking:",
                    "orderFrontSubstitutionsPanel:",
                    "toggleAutomaticQuoteSubstitution:", "uppercaseWord:",
                    "startSpeaking:",
                ]
            ),
            (
                3,
                [
                    "showLibrarySection:", "showImportSection:",
                    "showStorageManager:", "chooseLibraryBrowserMode:",
                    "fullWidthLibrary:", "focusSearch:", "goToNowPlaying:",
                    "toggleQueue:", "toggleFullScreen:",
                ]
            ),
            (
                4,
                [
                    "playPause:", "nextTrack:", "previousTrack:", "mute:",
                    "cycleRepeatMode:", "chooseRepeatMode:",
                    "pauseBetweenSides:", "restorePlaybackOnLaunch:",
                    "shuffleLibrary:",
                ]
            ),
            (5, ["performMiniaturize:", "performZoom:", "arrangeInFront:"]),
            (6, ["showHelp:"]),
        ]
    )
    func menuActions(index: Int, actions: [String]) throws {
        let menu = try #require(NSApp.mainMenu)
        MenuLookup.expect(try MenuLookup.submenu(menu, at: index), actions)
    }

    @Test("bae's key equivalents, and Select All's target")
    func keyEquivalents() throws {
        let menu = try #require(NSApp.mainMenu)
        let selectAll = try MenuLookup.item(menu, "selectAll:")
        #expect(selectAll.keyEquivalent == "a")
        #expect(selectAll.keyEquivalentModifierMask == .command)
        #expect(selectAll.target is SelectAllTarget)
        let playPause = try MenuLookup.item(menu, "playPause:")
        #expect(playPause.keyEquivalent == " ")
        #expect(playPause.keyEquivalentModifierMask == [])
        let search = try MenuLookup.item(menu, "focusSearch:")
        #expect(search.keyEquivalent == "/")
        #expect(search.keyEquivalentModifierMask == [])
        let closeLibrary = try MenuLookup.item(menu, "closeLibrary:")
        #expect(closeLibrary.keyEquivalent == "w")
        #expect(closeLibrary.keyEquivalentModifierMask == [.command, .shift])
    }

    /// The Edit menu is AppKit's to show: focus moving between a text field
    /// and a list, each validated as AppKit validates when the menu opens,
    /// leaves the menu holding the same items.
    @Test("the Edit menu keeps its items as focus moves")
    func editMenuItemsHoldAcrossFocus() async throws {
        let menu = try #require(NSApp.mainMenu)
        let edit = try MenuLookup.submenu(menu, at: 2)
        let size = NSSize(width: 400, height: 320)
        try await SnapshotTestSupport.withHostedWindow(
            FocusFixture().frame(width: size.width, height: size.height),
            size: size
        ) { window, host in
            try await SnapshotTestSupport.settle(host)
            let views = SnapshotTestSupport.descendants(of: host)
            let field = try #require(
                views.compactMap { $0 as? NSTextField }.first
            )
            let table = try #require(
                views.compactMap { $0 as? NSTableView }.first
            )
            edit.update()
            let items = edit.items.map(ObjectIdentifier.init)

            for responder in [field, table, field] as [NSView] {
                #expect(window.makeFirstResponder(responder))
                try await SnapshotTestSupport.settle(host)
                edit.update()
                #expect(edit.items.map(ObjectIdentifier.init) == items)
            }
        }
    }
}

/// A text field above a list, for focus to move between.
private struct FocusFixture: View {
    @State
    private var text = "Album Title"
    @State
    private var selection: Set<Int> = []

    var body: some View {
        VStack {
            TextField(text: $text) { Text(verbatim: "Field") }
            List(0..<3, id: \.self, selection: $selection) { row in
                Text(verbatim: "Row \(row)")
            }
        }
    }
}
