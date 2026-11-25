#if canImport(AppKit)
    import AppKit
    import OSLog
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

        /// The panel keeps the frame the menu gave it. Its clear room for
        /// the shadow makes a tall menu's panel taller than the screen's
        /// visible frame, and AppKit would move the face with it.
        override func constrainFrameRect(
            _ frameRect: NSRect,
            to screen: NSScreen?
        ) -> NSRect {
            frameRect
        }
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

        /// The line under a screen point, so a line is what an assistive
        /// technology or a UI test finds there and reaches. Without it the
        /// point answers with the menu, and a UI test calls the line it can
        /// see not hittable.
        override func accessibilityHitTest(_ point: NSPoint) -> Any? {
            items.first { $0.accessibilityFrame().contains(point) }
                ?? super.accessibilityHitTest(point)
        }
    }

    /// One open menu or submenu.
    private struct OpenLevel {
        let model: MacMenuLevel
        let panel: MacMenuPanel
        let host: MenuHostingView
        /// The tallest the face can be on the screen `position` chose.
        let tallest: CGFloat
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
        /// The screen a menu goes on when its position is on none.
        private let fallbackScreen: NSScreen
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
        @discardableResult
        static func present(
            _ entries: [MacMenuEntry],
            over parent: NSWindow,
            placement: MacMenuPlacement,
            appearance: HostedAppearance,
            isPopUp: Bool,
            onClose: @escaping @MainActor () -> Void = {}
        ) -> MacMenuSession? {
            current?.close()
            guard !entries.isEmpty else { return nil }
            // With no screen to draw on, as while displays reconnect, no
            // menu opens.
            guard let screen = parent.screen ?? NSScreen.main else {
                Logger.bae("menu")
                    .warning(
                        "no menu opens over the window \"\(parent.title)\": it and the app have no screen"
                    )
                return nil
            }
            let session = MacMenuSession(
                parent: parent,
                screen: screen,
                appearance: appearance,
                isPopUp: isPopUp,
                onClose: onClose
            )
            current = session
            session.open(entries, placement: placement)
            return session
        }

        private init(
            parent: NSWindow,
            screen: NSScreen,
            appearance: HostedAppearance,
            isPopUp: Bool,
            onClose: @escaping @MainActor () -> Void
        ) {
            self.parent = parent
            self.fallbackScreen = screen
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
                reveal(level: 0)
            }
            parent.addChildWindow(level.panel, ordered: .above)
            level.panel.makeKeyAndOrderFront(nil)
            listen()
            // Assistive technologies and UI tests wait for this to know a
            // menu opened, as they do for an NSMenu.
            if let content = level.panel.contentView {
                NSAccessibility.post(
                    element: content,
                    notification: .init(rawValue: "AXMenuOpened")
                )
            }
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
            let (face, tallest) = position(
                size,
                placement: placement,
                entries: entries
            )
            model.scrollingHeight = MacMenuMetrics.scrollingHeight(
                of: entries,
                tallest: tallest
            )
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
                size: face.size,
                level: index,
                host: host,
                model: model
            )
            return OpenLevel(
                model: model,
                panel: panel,
                host: host,
                tallest: tallest,
                face: face
            )
        }

        /// The face's frame in screen coordinates, kept on the screen, with
        /// the tallest face the screen allows. A face taller than the
        /// screen's visible frame is cut to fit it, and scrolls its lines.
        private func position(
            _ fullSize: NSSize,
            placement: MacMenuPlacement,
            entries: [MacMenuEntry]
        ) -> (face: NSRect, tallest: CGFloat) {
            func originFor(height: CGFloat) -> NSPoint {
                switch placement {
                case .below(let rect):
                    return NSPoint(x: rect.minX, y: rect.minY - 2 - height)
                case .over(let rect, let chosen):
                    let frames = MacMenuMetrics.frames(
                        of: entries,
                        width: fullSize.width
                    )
                    let down = chosen.map { frames[$0].midY } ?? 0
                    return NSPoint(
                        x: rect.minX - MacMenuMetrics.wordsInset,
                        y: rect.midY + down - height
                    )
                case .at(let point):
                    return NSPoint(x: point.x, y: point.y - height)
                }
            }
            var size = fullSize
            var origin = originFor(height: size.height)
            let screen =
                NSScreen.screens.first {
                    $0.frame.contains(NSPoint(x: origin.x, y: origin.y))
                        || $0.frame.contains(NSEvent.mouseLocation)
                } ?? fallbackScreen
            let visible = screen.visibleFrame
            let tallest = MacMenuMetrics.tallestFace(onScreenOf: visible.height)
            size.height = MacMenuMetrics.faceHeight(
                of: entries,
                tallest: tallest
            )
            origin = originFor(height: size.height)
            // Flip above the anchor, or up from the point, when there is no
            // room below.
            if origin.y < visible.minY, case .below(let rect) = placement {
                let above = rect.maxY + 2
                if above + size.height <= visible.maxY { origin.y = above }
            }
            else if origin.y < visible.minY, case .at(let point) = placement {
                origin.y = point.y
            }
            let margin = MacMenuMetrics.screenMargin
            origin.x = min(origin.x, visible.maxX - size.width - margin)
            origin.x = max(origin.x, visible.minX + margin)
            origin.y = min(origin.y, visible.maxY - size.height - margin)
            origin.y = max(origin.y, visible.minY + margin)
            return (NSRect(origin: origin, size: size), tallest)
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
                // AppKit reads an element's frame in its parent's space with
                // y going up, though the hosting view is flipped: measured
                // from the top, each line would sit where the menu draws
                // another line, and a click on it would pick that one.
                element.setAccessibilityFrameInParentSpace(
                    NSRect(
                        x: room + frame.minX,
                        y: room + size.height
                            - (frame.maxY - model.scrollOffset),
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
        /// Whether this menu is the one open.
        var isOpen: Bool { Self.current === self }

        /// Shows `entries` in place of the menu's own lines while it is
        /// open, the highlight where it was; the menu's top stays put and
        /// its height follows.
        func update(_ entries: [MacMenuEntry]) {
            guard isOpen, let level = levels.first else { return }
            closeLevels(after: 0)
            level.model.entries = entries
            if level.model.highlighted.map({ $0 >= entries.count }) ?? false {
                level.model.highlighted = nil
            }
            let height = MacMenuMetrics.faceHeight(
                of: entries,
                tallest: level.tallest
            )
            level.model.scrollingHeight = MacMenuMetrics.scrollingHeight(
                of: entries,
                tallest: level.tallest
            )
            level.model.scrollOffset = MacMenuMetrics.clampedScroll(
                level.model.scrollOffset,
                of: entries,
                faceHeight: height
            )
            var face = level.face
            face.origin.y += face.height - height
            face.size.height = height
            let room = MacMenuMetrics.shadowRoom
            level.panel.setFrame(
                face.insetBy(dx: -room, dy: -room),
                display: true
            )
            level.host.frame = NSRect(
                origin: .zero,
                size: level.panel.frame.size
            )
            let resized = OpenLevel(
                model: level.model,
                panel: level.panel,
                host: level.host,
                tallest: level.tallest,
                face: face
            )
            levels[0] = resized
            refreshAccessibilityItems(resized, level: 0)
        }

        // MARK: Listening

        private func listen() {
            eventMonitor = NSEvent.addLocalMonitorForEvents(
                matching: [
                    .keyDown, .leftMouseDown, .rightMouseDown, .otherMouseDown,
                    .scrollWheel,
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
            if event.type == .scrollWheel {
                return scroll(event) ? nil : event
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
            defer { reveal(level: levels.count - 1) }
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

        /// Scrolls level `index` so its highlighted line shows, and moves the
        /// lines' accessibility frames with it.
        private func reveal(level index: Int) {
            guard levels.indices.contains(index) else { return }
            let level = levels[index]
            let model = level.model
            if let row = model.highlighted, model.scrollingHeight != nil {
                model.scrollOffset = MacMenuMetrics.scroll(
                    toShow: row,
                    of: model.entries,
                    faceHeight: level.face.height,
                    from: model.scrollOffset
                )
            }
            refreshAccessibilityItems(level, level: index)
        }

        /// Gives VoiceOver the lines of `level`, the one at `index` in
        /// `levels`, where the face draws them.
        private func refreshAccessibilityItems(
            _ level: OpenLevel,
            level index: Int
        ) {
            level.host.items = accessibilityItems(
                level.model.entries,
                size: level.face.size,
                level: index,
                host: level.host,
                model: level.model
            )
        }

        /// Scrolls the face under the pointer by a wheel or trackpad event;
        /// false when the pointer is on no scrolling face.
        private func scroll(_ event: NSEvent) -> Bool {
            let point = NSEvent.mouseLocation
            guard
                let index = levels.lastIndex(where: { $0.face.contains(point) }
                ),
                levels[index].model.scrollingHeight != nil
            else { return false }
            let level = levels[index]
            // Scrolling content down is a positive delta; a wheel's notches
            // count lines.
            let delta =
                event.hasPreciseScrollingDeltas
                ? event.scrollingDeltaY
                : event.scrollingDeltaY * MacMenuMetrics.itemHeight
            level.model.scrollOffset = MacMenuMetrics.clampedScroll(
                level.model.scrollOffset - delta,
                of: level.model.entries,
                faceHeight: level.face.height
            )
            refreshAccessibilityItems(level, level: index)
            return true
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
                case .submenu(_, let entries, _) = levels[level].model
                    .entries[row]
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
                parentLevel.face.maxY
                - (rowFrame.minY - parentLevel.model.scrollOffset)
                + MacMenuMetrics.padding
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
            reveal(level: levels.count - 1)
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
            return OpenLevel(
                model: level.model,
                panel: level.panel,
                host: level.host,
                tallest: level.tallest,
                face: face
            )
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
            if let content = levels.first?.panel.contentView {
                NSAccessibility.post(
                    element: content,
                    notification: .init(rawValue: "AXMenuClosed")
                )
            }
            for level in levels.reversed() { dismiss(level) }
            levels = []
            if hadKey, parent.isVisible, NSApp.isActive {
                parent.makeKey()
            }
            onClose()
        }
    }
#endif
