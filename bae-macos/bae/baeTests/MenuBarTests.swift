import AppKit
import Foundation
import Observation
import Testing

@testable import bae

/// The menu bar's state follows what it reads at the end of each pass of the
/// run loop while no menu is open, and holds what an open menu was built
/// from until the menu closes.
@MainActor
@Suite("What the menu bar shows, held still while a menu is open")
struct MenuBarTests {
    private let copy = #selector(NSText.copy(_:))

    @Test(
        "a change nothing announces shows at the end of the pass that made it"
    )
    func readsAtTheEndOfEachPass() {
        let fixture = Fixture()

        // What a command sent by its key equivalent does: AppKit sends the
        // action and posts no window update after it.
        fixture.asked.clipboard = [copy]
        #expect(fixture.menuBar.state.clipboard.isEmpty)
        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard == [copy])
    }

    @Test("what a pass changed is in place before SwiftUI renders that pass")
    func writesBeforeSwiftUIRenders() {
        let fixture = Fixture()
        var rendered: [Set<Selector>] = []
        fixture.whenSwiftUIRenders {
            rendered.append(fixture.menuBar.state.clipboard)
        }

        fixture.asked.clipboard = [copy]
        fixture.runPass()
        #expect(rendered == [[copy]])
    }

    @Test("a change made while SwiftUI renders asks for one more pass")
    func aChangeDuringTheRenderAsksForAnotherPass() {
        let fixture = Fixture()
        fixture.whenSwiftUIRenders {
            // As focus moves to a field a command asked for.
            fixture.asked.clipboard = [copy]
        }

        fixture.runPass()
        #expect(fixture.wakeUps == 1)

        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard == [copy])
        #expect(fixture.wakeUps == 1, "one is enough")
    }

    @Test("nothing is read while a menu is open, and it catches up once closed")
    func holdsWhileAMenuIsOpen() {
        let fixture = Fixture()
        let menu = NSMenu()

        fixture.post(NSMenu.didBeginTrackingNotification, menu)
        fixture.asked.clipboard = [copy]
        fixture.runPass()
        #expect(
            fixture.menuBar.state.clipboard.isEmpty,
            "the open menu is not rebuilt"
        )
        #expect(fixture.wakeUps == 0, "nor is another pass asked for")

        fixture.post(NSMenu.didEndTrackingNotification, menu)
        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard == [copy])
    }

    @Test("a menu open inside another holds until the outer one closes")
    func holdsUntilTheLastMenuCloses() {
        let fixture = Fixture()
        let outer = NSMenu()
        let inner = NSMenu()

        fixture.post(NSMenu.didBeginTrackingNotification, outer)
        fixture.post(NSMenu.didBeginTrackingNotification, inner)
        fixture.asked.clipboard = [copy]
        fixture.post(NSMenu.didEndTrackingNotification, inner)
        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard.isEmpty)

        fixture.post(NSMenu.didEndTrackingNotification, outer)
        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard == [copy])
    }

    @Test("a menu that ends without beginning doesn't count against the next")
    func anUnbalancedEndIsNotCounted() {
        let fixture = Fixture()
        let menu = NSMenu()

        fixture.post(NSMenu.didEndTrackingNotification, menu)
        fixture.post(NSMenu.didBeginTrackingNotification, menu)
        fixture.asked.clipboard = [copy]
        fixture.runPass()
        #expect(fixture.menuBar.state.clipboard.isEmpty)
    }

    @Test("reading nothing new leaves the commands' input untouched")
    func anUnchangedReadIsNoChange() {
        let fixture = Fixture()
        let changes = ChangeCount()
        withObservationTracking {
            _ = fixture.menuBar.state
        } onChange: {
            changes.record()
        }

        fixture.runPass()
        #expect(changes.count == 0)

        fixture.asked.clipboard = [copy]
        fixture.runPass()
        #expect(changes.count == 1)
    }

    @Test("a menu bar no longer held reads nothing more")
    func releasingStopsReading() {
        let fixture = Fixture()
        var reads = 0
        weak var released: MenuBar?
        do {
            let menuBar = fixture.makeMenuBar { reads += 1 }
            released = menuBar
            fixture.runPass()
        }
        #expect(released == nil)
        let readsWhileHeld = reads

        fixture.runPass()
        #expect(reads == readsWhileHeld)
    }
}

/// A menu bar over a value a test sets, reading on the passes of the main
/// run loop in a mode of its own, so a test runs those passes alone.
@MainActor
private final class Fixture {
    let asked = AskedValues()
    let notifications = NotificationCenter()
    private(set) var menuBar: MenuBar!
    private let mode = CFRunLoopMode(
        "MenuBarTests.\(UUID().uuidString)" as CFString
    )
    private let runLoop: CFRunLoop = CFRunLoopGetMain()
    /// A run loop mode with nothing in it doesn't run, so a timer that never
    /// fires keeps it running for as long as a test asks.
    private let keepsTheModeRunning: CFRunLoopTimer =
        CFRunLoopTimerCreateWithHandler(
            nil,
            CFAbsoluteTimeGetCurrent() + 1e9,
            0,
            0,
            0
        ) { _ in }
    private var renders: [CFRunLoopObserver] = []
    /// How many more passes the menu bar asked for. Not passed on to the
    /// run loop: other work wakes it too, so a test can't tell its passes
    /// apart.
    private(set) var wakeUps = 0

    init() {
        CFRunLoopAddTimer(runLoop, keepsTheModeRunning, mode)
        menuBar = makeMenuBar {}
    }

    isolated deinit {
        CFRunLoopTimerInvalidate(keepsTheModeRunning)
        for observer in renders {
            CFRunLoopObserverInvalidate(observer)
        }
    }

    /// A menu bar over `asked`, calling `onRead` on each read.
    func makeMenuBar(onRead: @escaping @MainActor () -> Void) -> MenuBar {
        let asked = asked
        return MenuBar(
            notifications: notifications,
            runLoop: runLoop,
            mode: mode,
            wakeUp: { [weak self] _ in self?.wakeUps += 1 },
            read: {
                onRead()
                return MenuBarState(
                    canCheckForUpdates: false,
                    libraries: [],
                    clipboard: asked.clipboard,
                    restoresPlaybackOnLaunch: false,
                    library: nil
                )
            }
        )
    }

    /// Calls `render` where SwiftUI renders the main menu: its run loop
    /// observer of order 0, before the loop waits.
    func whenSwiftUIRenders(_ render: @escaping @MainActor () -> Void) {
        let observer: CFRunLoopObserver = CFRunLoopObserverCreateWithHandler(
            nil,
            CFRunLoopActivity.beforeWaiting.rawValue,
            true,
            0
        ) { _, _ in
            MainActor.assumeIsolated(render)
        }
        CFRunLoopAddObserver(runLoop, observer, mode)
        renders.append(observer)
    }

    /// Runs the run loop for a moment: one pass, and any more something
    /// else wakes it for.
    func runPass() {
        CFRunLoopRunInMode(mode, 0.01, false)
    }

    func post(_ name: Notification.Name, _ object: Any? = nil) {
        notifications.post(name: name, object: object)
    }
}

/// Like what the first responder can do: read when asked, never observed.
@MainActor
private final class AskedValues {
    var clipboard: Set<Selector> = []
}

private final class ChangeCount: @unchecked Sendable {
    private let lock = NSLock()
    private var recorded = 0

    var count: Int { lock.withLock { recorded } }

    func record() {
        lock.withLock { recorded += 1 }
    }
}
