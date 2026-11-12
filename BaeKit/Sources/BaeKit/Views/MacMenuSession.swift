#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// Where a menu opens.
    enum MacMenuPlacement {
        /// Under the anchor, its left edges together.
        case below(anchor: NSRect)
        /// Over the anchor as a pop-up opens: the line `chosen` on the
        /// anchor's line, the words of its lines over the anchor's value.
        case over(anchor: NSRect, chosen: Int?)
        /// Its top left corner at a point, as a context menu opens.
        case at(NSPoint)
    }

    /// A borderless panel that can take the keyboard.
    private final class MacMenuPanel: NSPanel {
        override var canBecomeKey: Bool { true }
    }

    /// A menu line as VoiceOver reads it.
    private final class MenuItemElement: NSAccessibilityElement {
        nonisolated(unsafe) var press: (@MainActor () -> Void)?
        nonisolated(unsafe) var isHighlighted: (@MainActor () -> Bool)?

        override func accessibilityPerformPress() -> Bool {
            let press = press
            MainActor.assumeIsolated { press?() }
            return true
        }

        override func isAccessibilitySelected() -> Bool {
            let isHighlighted = isHighlighted
            return MainActor.assumeIsolated { isHighlighted?() ?? false }
        }
    }

    /// The panel's content: a menu to VoiceOver, its lines menu items.
    private final class MenuHostingView: NSHostingView<MacMenuPanelContent> {
        var items: [MenuItemElement] = []

        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func isAccessibilityElement() -> Bool { true }

        override func accessibilityRole() -> NSAccessibility.Role? { .menu }

        override func accessibilityChildren() -> [Any]? { items }
    }

    /// One open menu or submenu.
    private struct OpenLevel {
        let model: MacMenuLevel
        let panel: MacMenuPanel
        /// The face, in screen coordinates.
        let face: NSRect
    }

    /// An open bae menu: its panels, the events it listens to while it is
    /// open, and what closes it. One is open at a time.
    ///
    /// The panel is a clear borderless window attached to the window it
    /// opened from, holding bae's face. Keys are read from the event stream
    /// and not from the views' focus, so the arrows, Return, Escape and
    /// typed letters work the moment it opens. A click outside the menu
    /// closes it and goes no further, as NSMenu does; so does the window
    /// losing the keyboard.
    @MainActor
    final class MacMenuSession {
        private static var current: MacMenuSession?

        private let parent: NSWindow
        private let appearance: HostedAppearance
        private let colorScheme: ColorScheme
        private let isPopUp: Bool
        private var levels: [OpenLevel] = []
        private var eventMonitor: Any?
        private var observers: [NSObjectProtocol] = []
        private var typed = ""
        private var typedAt = Date.distantPast
        private var hoverTask: Task<Void, Never>?
        /// For each open submenu, the line of its parent that opened it.
        private var openSubmenuRows: [Int] = []
        private let onClose: @MainActor () -> Void

        /// Opens `entries` over `parent`, closing any menu that was open.
        static func present(
            _ entries: [MacMenuEntry],
            over parent: NSWindow,
            placement: MacMenuPlacement,
            appearance: HostedAppearance,
            isPopUp: Bool,
            onClose: @escaping @MainActor () -> Void = {}
        ) {
            current?.close()
            guard !entries.isEmpty else { return }
            let session = MacMenuSession(
                parent: parent,
                appearance: appearance,
                isPopUp: isPopUp,
                onClose: onClose
            )
            current = session
            session.open(entries, placement: placement)
        }

        static var isOpen: Bool { current != nil }

        private init(
            parent: NSWindow,
            appearance: HostedAppearance,
            isPopUp: Bool,
            onClose: @escaping @MainActor () -> Void
        ) {
            self.parent = parent
            self.appearance = appearance
            self.isPopUp = isPopUp
            self.onClose = onClose
            let match = parent.effectiveAppearance.bestMatch(
                from: [.aqua, .darkAqua]
            )
            self.colorScheme = match == .darkAqua ? .dark : .light
        }

        // MARK: Opening

        private func open(
            _ entries: [MacMenuEntry],
            placement: MacMenuPlacement
        ) {
            let level = makeLevel(entries, placement: placement)
            levels = [level]
            if case .over(_, let chosen) = placement {
                level.model.highlighted = chosen
            }
            parent.addChildWindow(level.panel, ordered: .above)
            level.panel.makeKeyAndOrderFront(nil)
            listen()
        }

        private func makeLevel(
            _ entries: [MacMenuEntry],
            placement: MacMenuPlacement
        ) -> OpenLevel {
            let model = MacMenuLevel(entries: entries)
            let index = levels.count
            let host = MenuHostingView(
                rootView: MacMenuPanelContent(
                    level: model,
                    isPopUp: isPopUp,
                    appearance: appearance,
                    colorScheme: colorScheme,
                    hover: { [weak self] row, inside in
                        self?.hovered(row, inside: inside, level: index)
                    },
                    pick: { [weak self] row in
                        self?.pick(row, level: index)
                    }
                )
            )
            let room = MacMenuMetrics.shadowRoom
            let fitting = host.fittingSize
            let size = NSSize(
                width: fitting.width - 2 * room,
                height: MacMenuMetrics.faceHeight(of: entries)
            )
            let face = position(size, placement: placement, entries: entries)
            let panel = MacMenuPanel(
                contentRect: face.insetBy(dx: -room, dy: -room),
                styleMask: [.borderless, .nonactivatingPanel],
                backing: .buffered,
                defer: false
            )
            panel.isReleasedWhenClosed = false
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.animationBehavior = .none
            panel.appearance = parent.effectiveAppearance
            panel.contentView = host
            host.frame = NSRect(origin: .zero, size: panel.frame.size)
            host.items = accessibilityItems(
                entries,
                size: size,
                level: index,
                host: host,
                model: model
            )
            return OpenLevel(model: model, panel: panel, face: face)
        }

        /// The face's frame in screen coordinates, kept on the screen.
        private func position(
            _ size: NSSize,
            placement: MacMenuPlacement,
            entries: [MacMenuEntry]
        ) -> NSRect {
            var origin: NSPoint
            switch placement {
            case .below(let rect):
                origin = NSPoint(x: rect.minX, y: rect.minY - 2 - size.height)
            case .over(let rect, let chosen):
                let frames = MacMenuMetrics.frames(
                    of: entries,
                    width: size.width
                )
                let down = chosen.map { frames[$0].midY } ?? 0
                origin = NSPoint(
                    x: rect.minX - MacMenuMetrics.wordsInset,
                    y: rect.midY + down - size.height
                )
            case .at(let point):
                origin = NSPoint(x: point.x, y: point.y - size.height)
            }
            let screen =
                NSScreen.screens.first {
                    $0.frame.contains(NSPoint(x: origin.x, y: origin.y))
                        || $0.frame.contains(NSEvent.mouseLocation)
                } ?? parent.screen ?? NSScreen.main
            if let visible = screen?.visibleFrame {
                // Flip above the anchor, or up from the point, when there is
                // no room below.
                if origin.y < visible.minY, case .below(let rect) = placement {
                    let above = rect.maxY + 2
                    if above + size.height <= visible.maxY { origin.y = above }
                }
                else if origin.y < visible.minY, case .at(let point) = placement
                {
                    origin.y = point.y
                }
                origin.x = min(origin.x, visible.maxX - size.width - 4)
                origin.x = max(origin.x, visible.minX + 4)
                origin.y = min(origin.y, visible.maxY - size.height - 4)
                origin.y = max(origin.y, visible.minY + 4)
            }
            return NSRect(origin: origin, size: size)
        }

        private func accessibilityItems(
            _ entries: [MacMenuEntry],
            size: NSSize,
            level: Int,
            host: NSView,
            model: MacMenuLevel
        ) -> [MenuItemElement] {
            let room = MacMenuMetrics.shadowRoom
            let frames = MacMenuMetrics.frames(of: entries, width: size.width)
            var elements: [MenuItemElement] = []
            for (index, entry) in entries.enumerated() {
                guard let title = entry.title else { continue }
                let element = MenuItemElement()
                element.setAccessibilityRole(.menuItem)
                element.setAccessibilityLabel(title)
                element.setAccessibilityParent(host)
                if case .item(let item) = entry {
                    element.setAccessibilityEnabled(!item.isDisabled)
                    if item.isChecked { element.setAccessibilityValue(1) }
                }
                let frame = frames[index]
                let y =
                    host.isFlipped
                    ? room + frame.minY
                    : room + size.height - frame.maxY
                element.setAccessibilityFrameInParentSpace(
                    NSRect(
                        x: room + frame.minX,
                        y: y,
                        width: frame.width,
                        height: frame.height
                    )
                )
                element.press = { [weak self] in
                    self?.pick(index, level: level)
                }
                element.isHighlighted = { [weak model] in
                    model?.highlighted == index
                }
                elements.append(element)
            }
            return elements
        }

    }

    extension MacMenuSession {
        // MARK: Listening

        private func listen() {
            eventMonitor = NSEvent.addLocalMonitorForEvents(
                matching: [
                    .keyDown, .leftMouseDown, .rightMouseDown, .otherMouseDown,
                ]
            ) { [weak self] event in
                guard let self else { return event }
                nonisolated(unsafe) var result: NSEvent?
                nonisolated(unsafe) let incoming = event
                MainActor.assumeIsolated { result = self.handle(incoming) }
                return result
            }
            let center = NotificationCenter.default
            observers.append(
                center.addObserver(
                    forName: NSWindow.didResignKeyNotification,
                    object: nil,
                    queue: .main
                ) { [weak self] note in
                    let window = note.object as? NSWindow
                    MainActor.assumeIsolated {
                        guard let self, self.owns(window) else { return }
                        // The keyboard moves between the menu's own panels
                        // before it moves away: look after it has settled.
                        DispatchQueue.main.async {
                            MainActor.assumeIsolated {
                                self.closeIfKeyLeft()
                            }
                        }
                    }
                }
            )
            for name in [
                NSWindow.didMoveNotification, NSWindow.didResizeNotification,
                NSWindow.willCloseNotification,
            ] {
                observers.append(
                    center.addObserver(
                        forName: name,
                        object: parent,
                        queue: .main
                    ) { [weak self] _ in
                        MainActor.assumeIsolated { self?.close() }
                    }
                )
            }
        }

        private func owns(_ window: NSWindow?) -> Bool {
            window === parent || levels.contains { $0.panel === window }
        }

        private func closeIfKeyLeft() {
            if !levels.contains(where: { $0.panel.isKeyWindow }) { close() }
        }

        private func handle(_ event: NSEvent) -> NSEvent? {
            if event.type == .keyDown {
                if event.modifierFlags.contains(.command) { return event }
                key(event)
                return nil
            }
            // A click anywhere outside the menus' faces closes it, and goes
            // no further.
            let screenPoint =
                event.window?.convertPoint(toScreen: event.locationInWindow)
                ?? NSEvent.mouseLocation
            if levels.contains(where: { $0.face.contains(screenPoint) }) {
                return event
            }
            close()
            return nil
        }

        // MARK: Keys

        private func key(_ event: NSEvent) {
            guard let level = levels.last else { return }
            let model = level.model
            switch event.keyCode {
            case 126: move(model, by: -1)
            case 125: move(model, by: 1)
            case 36, 76: activate(model, level: levels.count - 1)
            case 49 where typed.isEmpty:
                activate(model, level: levels.count - 1)
            case 53: close()
            case 124:
                if let row = model.highlighted,
                    case .submenu = model.entries[row]
                {
                    openSubmenu(row, level: levels.count - 1)
                }
            case 123:
                if levels.count > 1 { closeLevels(after: levels.count - 2) }
            case 115: highlight(model, first: true)
            case 119: highlight(model, first: false)
            default:
                if let characters = event.characters, !characters.isEmpty,
                    !event.modifierFlags.contains(.control)
                {
                    typeToSelect(characters, in: model)
                }
            }
        }

        private func move(_ model: MacMenuLevel, by step: Int) {
            let count = model.entries.count
            var index = model.highlighted ?? (step > 0 ? -1 : count)
            for _ in 0..<count {
                index = (index + step + count) % count
                if model.entries[index].isChoosable {
                    model.highlighted = index
                    return
                }
            }
        }

        private func highlight(_ model: MacMenuLevel, first: Bool) {
            let indices = model.entries.indices.filter {
                model.entries[$0].isChoosable
            }
            model.highlighted = first ? indices.first : indices.last
        }

        private func activate(_ model: MacMenuLevel, level: Int) {
            guard let row = model.highlighted else { return }
            pick(row, level: level)
        }

        private func typeToSelect(_ characters: String, in model: MacMenuLevel)
        {
            if Date().timeIntervalSince(typedAt) > 1 { typed = "" }
            typedAt = Date()
            typed += characters.lowercased()
            let match = model.entries.indices.first { index in
                model.entries[index].isChoosable
                    && (model.entries[index].title ?? "").lowercased()
                        .hasPrefix(typed)
            }
            if let match { model.highlighted = match }
        }

        // MARK: Picking and submenus

        private func pick(_ row: Int, level: Int) {
            guard level < levels.count else { return }
            switch levels[level].model.entries[row] {
            case .item(let item):
                guard !item.isDisabled else { return }
                close()
                item.action()
            case .submenu:
                openSubmenu(row, level: level)
            case .divider, .header:
                break
            }
        }

        private func hovered(_ row: Int, inside: Bool, level: Int) {
            guard level < levels.count else { return }
            let model = levels[level].model
            guard inside else {
                if model.highlighted == row, !isOpenSubmenu(row, of: level) {
                    model.highlighted = nil
                }
                return
            }
            guard model.entries[row].isChoosable else { return }
            model.highlighted = row
            hoverTask?.cancel()
            if case .submenu = model.entries[row] {
                if !isOpenSubmenu(row, of: level) {
                    hoverTask = Task { @MainActor [weak self] in
                        try? await Task.sleep(for: .milliseconds(180))
                        guard !Task.isCancelled else { return }
                        self?.openSubmenu(row, level: level)
                    }
                }
            }
            else {
                closeLevels(after: level)
            }
        }

        private func isOpenSubmenu(_ row: Int, of level: Int) -> Bool {
            levels.count > level + 1 && openSubmenuRows[level] == row
        }

        private func openSubmenu(_ row: Int, level: Int) {
            guard level < levels.count,
                case .submenu(_, let entries) = levels[level].model.entries[row]
            else { return }
            closeLevels(after: level)
            let parentLevel = levels[level]
            let frames = MacMenuMetrics.frames(
                of: parentLevel.model.entries,
                width: parentLevel.face.width
            )
            let rowFrame = frames[row]
            // The submenu's first line sits level with the row, its face
            // overlapping the parent's edge by a few points.
            let top =
                parentLevel.face.maxY - rowFrame.minY + MacMenuMetrics.padding
            let anchor = NSRect(
                x: parentLevel.face.maxX - 4,
                y: top - MacMenuMetrics.itemHeight,
                width: 0,
                height: MacMenuMetrics.itemHeight
            )
            let next = makeLevel(
                entries,
                placement: .at(NSPoint(x: anchor.minX, y: top))
            )
            // Open to the left when there is no room on the right.
            let placed = flippedIfCrowded(next, parentLevel: parentLevel)
            levels.append(placed)
            openSubmenuRows.append(row)
            if openSubmenuRows.count > levels.count - 1 {
                openSubmenuRows = Array(
                    openSubmenuRows.suffix(levels.count - 1)
                )
            }
            placed.model.highlighted = placed.model.entries.firstIndex {
                $0.isChoosable
            }
            parent.addChildWindow(placed.panel, ordered: .above)
            placed.panel.orderFront(nil)
        }

        private func flippedIfCrowded(
            _ level: OpenLevel,
            parentLevel: OpenLevel
        ) -> OpenLevel {
            guard let visible = (parent.screen ?? NSScreen.main)?.visibleFrame,
                level.face.maxX > visible.maxX - 4
                    || level.face.minX < parentLevel.face.maxX - 8
            else { return level }
            let room = MacMenuMetrics.shadowRoom
            let face = NSRect(
                x: parentLevel.face.minX - level.face.width + 4,
                y: level.face.minY,
                width: level.face.width,
                height: level.face.height
            )
            level.panel.setFrame(
                face.insetBy(dx: -room, dy: -room),
                display: true
            )
            return OpenLevel(model: level.model, panel: level.panel, face: face)
        }

        private func closeLevels(after index: Int) {
            hoverTask?.cancel()
            while levels.count > index + 1 {
                let level = levels.removeLast()
                dismiss(level)
            }
            openSubmenuRows = Array(openSubmenuRows.prefix(index))
        }

        private func dismiss(_ level: OpenLevel) {
            parent.removeChildWindow(level.panel)
            level.panel.orderOut(nil)
            level.panel.contentView = nil
        }

        // MARK: Closing

        func close() {
            guard Self.current === self else { return }
            Self.current = nil
            hoverTask?.cancel()
            if let eventMonitor { NSEvent.removeMonitor(eventMonitor) }
            eventMonitor = nil
            for observer in observers {
                NotificationCenter.default.removeObserver(observer)
            }
            observers = []
            let hadKey = levels.contains { $0.panel.isKeyWindow }
            for level in levels.reversed() { dismiss(level) }
            levels = []
            if hadKey, parent.isVisible, NSApp.isActive {
                parent.makeKey()
            }
            onClose()
        }
    }
#endif
