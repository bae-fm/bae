#if canImport(AppKit)
    import AppKit
    import SwiftUI

    /// A borderless panel that can take the keyboard when the popover asks
    /// for it.
    private final class MacPopoverPanel: NSPanel {
        var takesKeyboard = true

        override var canBecomeKey: Bool { takesKeyboard }
    }

    /// The panel's content, which tells the popover when its content has
    /// grown or shrunk so the panel follows.
    private final class MacPopoverHostingView: NSHostingView<AnyView> {
        var contentSizeChanged: (() -> Void)?

        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func invalidateIntrinsicContentSize() {
            super.invalidateIntrinsicContentSize()
            contentSizeChanged?()
        }
    }

    /// An open bae popover: a clear borderless panel attached to the window
    /// it opened from, holding bae's face with its arrow at the anchor.
    ///
    /// It closes as a transient popover does: a click outside it, Escape, its
    /// window moving or closing, or the keyboard leaving it. A click on the
    /// anchor closes it and goes no further, so the anchor's own button
    /// toggles it.
    @MainActor
    final class MacPopoverSession {
        private let parent: NSWindow
        private let anchor: MacMenuAnchor
        private let edge: Edge
        private let model: MacPopoverModel
        private let panel = MacPopoverPanel(
            contentRect: .zero,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        private let host: MacPopoverHostingView
        private var eventMonitor: Any?
        private var observers: [NSObjectProtocol] = []
        private var isOpen = true
        private let onClose: @MainActor () -> Void

        /// Opens a popover on `edge` of `anchor`: below it for `.bottom`,
        /// above it for `.top`, and so on. `content` is the whole of what
        /// the popover shows, in the environment it opened from.
        init(
            content: AnyView,
            over parent: NSWindow,
            anchor: MacMenuAnchor,
            edge: Edge,
            takesKeyboard: Bool,
            onClose: @escaping @MainActor () -> Void
        ) {
            self.parent = parent
            self.anchor = anchor
            self.edge = edge
            self.onClose = onClose
            let model = MacPopoverModel(
                arrowSide: Self.arrowSide(facing: edge),
                content: content
            )
            self.model = model
            host = MacPopoverHostingView(
                rootView: AnyView(MacPopoverFace(model: model))
            )
            panel.takesKeyboard = takesKeyboard
            panel.isReleasedWhenClosed = false
            panel.isOpaque = false
            panel.backgroundColor = .clear
            panel.hasShadow = false
            panel.animationBehavior = .none
            panel.appearance = parent.effectiveAppearance
            panel.contentView = host
            host.contentSizeChanged = { [weak self] in
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { self?.place() }
                }
            }
            place()
            parent.addChildWindow(panel, ordered: .above)
            if takesKeyboard {
                panel.makeKeyAndOrderFront(nil)
            }
            else {
                panel.orderFront(nil)
            }
            listen()
        }

        /// The side of the popover its arrow is on, for the side of the
        /// anchor the popover sits on.
        private static func arrowSide(facing edge: Edge) -> Edge {
            switch edge {
            case .top: .bottom
            case .bottom: .top
            case .leading: .trailing
            case .trailing: .leading
            }
        }

        /// Shows `content` in place of what the popover showed.
        func update(_ content: AnyView) {
            model.content = content
        }

        // MARK: Placing

        /// Sizes and places the panel from the content's size and the
        /// anchor's place.
        private func place() {
            guard isOpen, let anchorFrame = anchor.screenFrame else { return }
            let room = MacPopoverMetrics.shadowRoom
            let fitting = host.fittingSize
            let size = NSSize(
                width: fitting.width - 2 * room,
                height: fitting.height - 2 * room
            )
            guard size.width > 0, size.height > 0 else { return }
            let visible =
                (parent.screen ?? NSScreen.main)?.visibleFrame
                ?? NSRect(x: 0, y: 0, width: 4_000, height: 4_000)
            var side = edge
            var face = frame(size, side: side, anchor: anchorFrame)
            if side == .bottom, face.minY < visible.minY {
                side = .top
                face = frame(size, side: side, anchor: anchorFrame)
            }
            else if side == .top, face.maxY > visible.maxY {
                side = .bottom
                face = frame(size, side: side, anchor: anchorFrame)
            }
            face.origin.x = max(
                visible.minX + 8,
                min(face.minX, visible.maxX - size.width - 8)
            )
            face.origin.y = max(
                visible.minY + 8,
                min(face.minY, visible.maxY - size.height - 8)
            )
            model.arrowSide = Self.arrowSide(facing: side)
            model.arrowOffset = arrowOffset(face: face, anchor: anchorFrame)
            let target = face.insetBy(dx: -room, dy: -room)
            if panel.frame != target {
                panel.setFrame(target, display: true)
                host.frame = NSRect(origin: .zero, size: target.size)
            }
        }

        private func frame(_ size: NSSize, side: Edge, anchor: NSRect) -> NSRect
        {
            let gap = MacPopoverMetrics.gap
            switch side {
            case .bottom:
                return NSRect(
                    x: anchor.midX - size.width / 2,
                    y: anchor.minY - gap - size.height,
                    width: size.width,
                    height: size.height
                )
            case .top:
                return NSRect(
                    x: anchor.midX - size.width / 2,
                    y: anchor.maxY + gap,
                    width: size.width,
                    height: size.height
                )
            case .leading:
                return NSRect(
                    x: anchor.minX - gap - size.width,
                    y: anchor.midY - size.height / 2,
                    width: size.width,
                    height: size.height
                )
            case .trailing:
                return NSRect(
                    x: anchor.maxX + gap,
                    y: anchor.midY - size.height / 2,
                    width: size.width,
                    height: size.height
                )
            }
        }

        /// How far along its side the arrow stands, kept off the corners.
        private func arrowOffset(face: NSRect, anchor: NSRect) -> CGFloat {
            let margin =
                MacPopoverMetrics.cornerRadius + MacPopoverMetrics.arrowWidth
                / 2
                + 2
            switch model.arrowSide {
            case .top, .bottom:
                return min(
                    max(anchor.midX - face.minX, margin),
                    face.width - margin
                )
            case .leading, .trailing:
                return min(
                    max(face.maxY - anchor.midY, margin),
                    face.height - margin
                )
            }
        }

        // MARK: Closing

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
                    object: panel,
                    queue: .main
                ) { [weak self] _ in
                    // The keyboard may be passing to a window the popover
                    // opened; look after it has settled.
                    DispatchQueue.main.async {
                        MainActor.assumeIsolated {
                            guard let self, self.panel.takesKeyboard,
                                !self.panel.isKeyWindow,
                                self.panel.attachedSheet == nil,
                                NSApp.keyWindow?.sheetParent !== self.panel
                            else { return }
                            self.close()
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

        private func handle(_ event: NSEvent) -> NSEvent? {
            if event.type == .keyDown {
                guard panel.takesKeyboard, event.keyCode == 53,
                    panel.isKeyWindow
                else { return event }
                close()
                return nil
            }
            if event.window === panel { return event }
            let point =
                event.window?.convertPoint(toScreen: event.locationInWindow)
                ?? NSEvent.mouseLocation
            if panel.frame
                .insetBy(
                    dx: MacPopoverMetrics.shadowRoom,
                    dy: MacPopoverMetrics.shadowRoom
                )
                .contains(point)
            {
                return event
            }
            let onAnchor = anchor.screenFrame?.contains(point) ?? false
            close()
            return onAnchor ? nil : event
        }

        func close() {
            guard isOpen else { return }
            isOpen = false
            if let eventMonitor { NSEvent.removeMonitor(eventMonitor) }
            eventMonitor = nil
            for observer in observers {
                NotificationCenter.default.removeObserver(observer)
            }
            observers = []
            let hadKey = panel.isKeyWindow
            parent.removeChildWindow(panel)
            panel.orderOut(nil)
            panel.contentView = nil
            if hadKey, parent.isVisible, NSApp.isActive {
                parent.makeKey()
            }
            onClose()
        }
    }
#endif
