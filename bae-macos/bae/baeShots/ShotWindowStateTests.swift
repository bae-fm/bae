import AppKit
import Testing

@testable import bae

/// What a failure for a scene whose window the window server does not show
/// says about the window server, the displays and the display's holder.
struct ShotWindowStateTests {
    @Test
    func theFailureNamesTheWindowsTheDisplaysAndTheHolder() {
        let state = ShotWindowState(
            windows: [
                .init(
                    id: 41,
                    entry: .init(
                        id: 41,
                        frame: CGRect(x: 1958, y: 758, width: 308, height: 194),
                        layer: 0,
                        onScreen: false
                    )
                ),
                .init(id: 42, entry: nil),
            ],
            displays: [
                .init(
                    id: 1,
                    frame: CGRect(x: 0, y: 0, width: 1920, height: 1080),
                    scale: 2
                ),
                .init(
                    id: 7,
                    frame: CGRect(x: 1920, y: 0, width: 3200, height: 1200),
                    scale: 2
                ),
            ],
            shotDisplay: 7,
            holder: "slot 1 of the shared display, leased by pid 90"
        )
        let failure =
            ShotError.windowNotDrawn(
                "mac.playlists.delete",
                CGRect(x: 1958, y: 758, width: 308, height: 194),
                state
            )
            .description
        #expect(failure.contains("off the screen or empty. "))
        #expect(
            failure.contains(
                "window 41: window server frame (1958.0, 758.0, 308.0, 194.0), "
                    + "layer 0, not on the screen"
            ),
            "\(failure)"
        )
        #expect(
            failure.contains("window 42: the window server lists no entry")
        )
        #expect(failure.contains("display 1: frame"))
        #expect(
            failure.contains("display 7 (the shot display): frame"),
            "\(failure)"
        )
        #expect(
            failure.contains(
                "shot display held as: slot 1 of the shared display, "
                    + "leased by pid 90"
            )
        )
    }

    /// The window server lists a window of this process with its frame and
    /// layer, and `read` finds it by its number.
    @Test
    @MainActor
    func readFindsTheWindowServersEntryForAWindowOfThisProcess() async throws {
        let display = try await ShotDisplay.add()
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 120, height: 80),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.animationBehavior = .none
        window.orderFront(nil)
        defer { window.close() }
        let number = CGWindowID(window.windowNumber)
        let state = ShotWindowState.read(
            windowIDs: [number, 0xFFFF_FFF0],
            on: display
        )
        let listed = try #require(state.windows.first?.entry)
        #expect(listed.id == number)
        #expect(listed.frame.size == CGSize(width: 120, height: 80))
        #expect(state.windows.last?.entry == nil)
        #expect(state.displays.contains { $0.id == display.id })
        #expect(state.holder.contains("owned by this process"))
    }
}
