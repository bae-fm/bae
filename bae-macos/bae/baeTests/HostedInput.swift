import AppKit
import Testing

/// Input for a view hosted by `SnapshotTestSupport.withHostedWindow`: clicks,
/// keys, presses and focus changes, each made and handled inside an
/// autorelease pool of its own.
///
/// AppKit autoreleases the events it makes and handles, and each holds the
/// window it is addressed to and whatever it hit. Under Swift Testing
/// nothing drains the pool those land in until the whole run ends; made and
/// handled here, each is let go as soon as it has been delivered.
@MainActor
enum HostedInput {
    /// When the latest press sent to each hosted window happened.
    private static var lastPress: [ObjectIdentifier: ContinuousClock.Instant] =
        [:]

    /// When every gesture a press into `window` started has settled: a
    /// double-click interval after the latest press, or `nil` when none was
    /// sent. Forgets the window.
    static func gesturesSettle(in window: NSWindow) -> ContinuousClock.Instant?
    {
        lastPress.removeValue(forKey: ObjectIdentifier(window))?
            .advanced(
                by: .seconds(NSEvent.doubleClickInterval) + .milliseconds(100)
            )
    }

    private static func pressed(_ window: NSWindow) {
        lastPress[ObjectIdentifier(window)] = .now
    }

    /// A press and release of the left button at `point` in `window`'s
    /// coordinates, as the `count`th click of a sequence.
    static func click(
        at point: NSPoint,
        in window: NSWindow,
        count: Int = 1
    ) throws {
        try autoreleasepool {
            for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
                window.sendEvent(
                    try mouseEvent(type, at: point, in: window, count: count)
                )
            }
        }
    }

    /// A click at `point` in `view`'s own coordinates.
    static func click(
        at point: NSPoint,
        in view: NSView,
        count: Int = 1
    ) throws {
        let window = try #require(view.window)
        try click(
            at: view.convert(point, to: nil),
            in: window,
            count: count
        )
    }

    /// Press what is drawn at `point` in `window`'s coordinates: the enabled
    /// control there through its own action, since SwiftUI's native buttons
    /// run no mouse-tracking loop for a synthetic click, and a click where
    /// there is no control.
    static func press(at point: NSPoint, in window: NSWindow) throws {
        let content = try #require(window.contentView)
        let control = SnapshotTestSupport.descendants(of: content)
            .compactMap { $0 as? NSControl }
            .first {
                $0.isEnabled && $0.convert($0.bounds, to: nil).contains(point)
            }
        if let control {
            press(control)
        }
        else {
            try click(at: point, in: window)
        }
    }

    /// `control` pressed through its own action.
    static func press(_ control: NSControl) {
        if let window = control.window { pressed(window) }
        autoreleasepool { control.performClick(nil) }
    }

    /// The keys the tests press: what each types and which key it is.
    enum Key {
        case `return`, escape, space, rightArrow

        var characters: String {
            switch self {
            case .return: "\r"
            case .escape: "\u{1b}"
            case .space: " "
            case .rightArrow:
                String(
                    utf16CodeUnits: [UInt16(NSRightArrowFunctionKey)],
                    count: 1
                )
            }
        }

        var code: UInt16 {
            switch self {
            case .return: 36
            case .escape: 53
            case .space: 49
            case .rightArrow: 124
            }
        }
    }

    /// A press of `key` delivered to `window`.
    static func keyDown(_ key: Key, in window: NSWindow) throws {
        try autoreleasepool {
            window.sendEvent(try keyEvent(key, in: window))
        }
    }

    /// A press of `key` offered to `view` as a key equivalent, and whether
    /// it took it.
    @discardableResult
    static func keyEquivalent(_ key: Key, in view: NSView) throws -> Bool {
        let window = try #require(view.window)
        return try autoreleasepool {
            view.performKeyEquivalent(with: try keyEvent(key, in: window))
        }
    }

    /// `responder` made `window`'s first responder, or no responder at all
    /// for `nil`, and whether the window accepted it.
    @discardableResult
    static func focus(_ responder: NSResponder?, in window: NSWindow) -> Bool {
        autoreleasepool { window.makeFirstResponder(responder) }
    }

    /// The context menu `view` offers for a right click at `point` in its
    /// window's coordinates.
    static func contextMenu(at point: NSPoint, in view: NSView) throws
        -> NSMenu?
    {
        let window = try #require(view.window)
        return try autoreleasepool {
            view.menu(
                for: try mouseEvent(.rightMouseDown, at: point, in: window)
            )
        }
    }

    /// A press of `control` at `point` in its window's coordinates that
    /// runs the control's own mouse-tracking loop to its release, as a
    /// person's press of a slider does.
    ///
    /// The release is queued for the loop to take. A loop that ends without
    /// taking it — it also reads the real mouse, whose button is up — leaves
    /// it queued, and the app's own event handling would take it later,
    /// outside any pool here; so whatever release is left is taken off the
    /// queue. The last event the app took is kept as its current event, so
    /// a windowless one is taken last in its place.
    static func track(_ control: NSControl, at point: NSPoint) throws {
        let window = try #require(control.window)
        try autoreleasepool {
            NSApp.postEvent(
                try mouseEvent(.leftMouseUp, at: point, in: window),
                atStart: true
            )
            control.mouseDown(
                with: try mouseEvent(.leftMouseDown, at: point, in: window)
            )
            while NSApp.nextEvent(
                matching: .leftMouseUp,
                until: nil,
                inMode: .eventTracking,
                dequeue: true
            ) != nil {}
            let windowless = try #require(
                NSEvent.otherEvent(
                    with: .applicationDefined,
                    location: .zero,
                    modifierFlags: [],
                    timestamp: 0,
                    windowNumber: 0,
                    context: nil,
                    subtype: 0,
                    data1: 0,
                    data2: 0
                )
            )
            NSApp.postEvent(windowless, atStart: true)
            _ = NSApp.nextEvent(
                matching: .applicationDefined,
                until: nil,
                inMode: .default,
                dequeue: true
            )
        }
    }

    private static func mouseEvent(
        _ type: NSEvent.EventType,
        at point: NSPoint,
        in window: NSWindow,
        count: Int = 1
    ) throws -> NSEvent {
        pressed(window)
        return try #require(
            NSEvent.mouseEvent(
                with: type,
                location: point,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: 0,
                clickCount: count,
                pressure: type == .leftMouseUp ? 0 : 1
            )
        )
    }

    private static func keyEvent(_ key: Key, in window: NSWindow) throws
        -> NSEvent
    {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber,
                context: nil,
                characters: key.characters,
                charactersIgnoringModifiers: key.characters,
                isARepeat: false,
                keyCode: key.code
            )
        )
    }
}
