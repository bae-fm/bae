import AppKit
import Foundation
import Observation
import Testing

@testable import bae

/// The menu bar's state follows what it reads while no menu is open, and
/// holds what an open menu was built from until the menu closes.
@MainActor
@Suite("What the menu bar shows, held still while a menu is open")
struct MenuBarTests {
    private let copy = #selector(NSText.copy(_:))

    @Test("a change to an observed value shows once it has landed")
    func followsAnObservedChange() {
        let fixture = Fixture()

        fixture.observed.restoresPlayback = true
        #expect(!fixture.menuBar.state.restoresPlaybackOnLaunch)
        fixture.scheduled.run()
        #expect(fixture.menuBar.state.restoresPlaybackOnLaunch)

        fixture.observed.restoresPlayback = false
        fixture.scheduled.run()
        #expect(
            !fixture.menuBar.state.restoresPlaybackOnLaunch,
            "a second change is seen too"
        )
    }

    @Test("an observed change while a menu is open shows when it closes")
    func holdsAnObservedChangeWhileAMenuIsOpen() {
        let fixture = Fixture()
        let menu = NSMenu()

        fixture.post(NSMenu.didBeginTrackingNotification, menu)
        fixture.observed.restoresPlayback = true
        fixture.scheduled.run()
        fixture.post(NSApplication.didUpdateNotification)
        #expect(
            !fixture.menuBar.state.restoresPlaybackOnLaunch,
            "the open menu is not rebuilt"
        )

        fixture.post(NSMenu.didEndTrackingNotification, menu)
        #expect(fixture.menuBar.state.restoresPlaybackOnLaunch)
    }

    @Test(
        "what is asked rather than observed is asked again on AppKit's update, not while a menu is open"
    )
    func asksAgainOnUpdateUnlessAMenuIsOpen() {
        let fixture = Fixture()
        let menu = NSMenu()

        fixture.asked.clipboard = [copy]
        #expect(fixture.menuBar.state.clipboard.isEmpty)
        fixture.post(NSApplication.didUpdateNotification)
        #expect(fixture.menuBar.state.clipboard == [copy])

        fixture.post(NSMenu.didBeginTrackingNotification, menu)
        fixture.asked.clipboard = []
        fixture.post(NSApplication.didUpdateNotification)
        #expect(
            fixture.menuBar.state.clipboard == [copy],
            "the open menu is not rebuilt"
        )

        fixture.post(NSMenu.didEndTrackingNotification, menu)
        #expect(fixture.menuBar.state.clipboard.isEmpty)
    }

    @Test("a user default changing asks again")
    func asksAgainWhenADefaultChanges() {
        let fixture = Fixture()

        fixture.asked.clipboard = [copy]
        fixture.post(UserDefaults.didChangeNotification)
        #expect(fixture.menuBar.state.clipboard == [copy])
    }

    @Test("a menu open inside another holds until the outer one closes")
    func holdsUntilTheLastMenuCloses() {
        let fixture = Fixture()
        let outer = NSMenu()
        let inner = NSMenu()

        fixture.post(NSMenu.didBeginTrackingNotification, outer)
        fixture.post(NSMenu.didBeginTrackingNotification, inner)
        fixture.observed.restoresPlayback = true
        fixture.scheduled.run()
        fixture.post(NSMenu.didEndTrackingNotification, inner)
        #expect(!fixture.menuBar.state.restoresPlaybackOnLaunch)

        fixture.post(NSMenu.didEndTrackingNotification, outer)
        #expect(fixture.menuBar.state.restoresPlaybackOnLaunch)
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

        fixture.post(NSApplication.didUpdateNotification)
        fixture.observed.restoresPlayback = false
        fixture.scheduled.run()
        #expect(changes.count == 0)

        fixture.observed.restoresPlayback = true
        fixture.scheduled.run()
        #expect(changes.count == 1)
    }
}

/// A menu bar over values a test sets: one observed, one asked.
@MainActor
private struct Fixture {
    let observed = ObservedValues()
    let asked = AskedValues()
    let scheduled = ScheduledWork()
    let notifications = NotificationCenter()
    let menuBar: MenuBar

    init() {
        let observed = observed
        let asked = asked
        menuBar = MenuBar(
            notifications: notifications,
            schedule: scheduled.schedule
        ) {
            MenuBarState(
                canCheckForUpdates: false,
                libraries: [],
                clipboard: asked.clipboard,
                restoresPlaybackOnLaunch: observed.restoresPlayback,
                library: nil
            )
        }
    }

    func post(_ name: Notification.Name, _ object: Any? = nil) {
        notifications.post(name: name, object: object)
    }
}

@MainActor
@Observable
private final class ObservedValues {
    var restoresPlayback = false
}

/// Like what the first responder can do: read when asked, never observed.
@MainActor
private final class AskedValues {
    var clipboard: Set<Selector> = []
}

/// Work the menu bar scheduled after an observed change, run when the test
/// says the change has landed.
private final class ScheduledWork: @unchecked Sendable {
    private let lock = NSLock()
    private var pending: [@MainActor @Sendable () -> Void] = []

    var schedule: MenuBarScheduler {
        { [self] work in lock.withLock { pending.append(work) } }
    }

    @MainActor
    func run() {
        let work = lock.withLock {
            let work = pending
            pending = []
            return work
        }
        for item in work {
            item()
        }
    }
}

private final class ChangeCount: @unchecked Sendable {
    private let lock = NSLock()
    private var recorded = 0

    var count: Int { lock.withLock { recorded } }

    func record() {
        lock.withLock { recorded += 1 }
    }
}
