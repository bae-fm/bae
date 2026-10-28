#if os(macOS)
    import AppKit

    /// A native view's reading of one key of a fast stream on the display's
    /// own clock: while the key is active, the view's display link reads the
    /// key's latest value each frame and shows it if it changed; while the
    /// key is idle, nothing runs. Values arriving faster than frames are never
    /// all shown; the screen shows the latest one each frame.
    @MainActor
    public final class FrameFollowing<Value: Sendable & Equatable> {
        private weak var view: NSView?
        private let show: (Value?) -> Void
        private var source: LatestSource<Value>?
        private var watch: LatestByKey<Value>.Watch?
        private var link: CADisplayLink?
        private lazy var ticker = FrameTicker { [weak self] in
            self?.showLatest()
        }
        /// What was last shown; `.none` until something is.
        private var shown: Value??

        /// Follow on `view`'s display, showing each new value with `show`.
        public init(view: NSView, show: @escaping (Value?) -> Void) {
            self.view = view
            self.show = show
        }

        /// Read `source` from now on; a source for the key already read
        /// changes nothing.
        public func follow(_ source: LatestSource<Value>) {
            if let current = self.source, current.isSame(as: source) { return }
            stop()
            self.source = source
            watch = source.store.watch(source.key) { [weak self] in
                self?.activityChanged()
            }
            activityChanged()
        }

        public func stop() {
            if let source, let watch {
                source.store.stopWatching(watch)
            }
            link?.invalidate()
            link = nil
            source = nil
            watch = nil
            shown = .none
        }

        /// Start reading each frame as the key starts, and stop as it goes
        /// idle, showing what it holds either way.
        private func activityChanged() {
            showLatest()
            let active = source?.latest != nil
            if active, link == nil, let view {
                let link = view.displayLink(
                    target: ticker,
                    selector: #selector(FrameTicker.frame(_:))
                )
                link.add(to: .main, forMode: .common)
                self.link = link
            }
            else if !active {
                link?.invalidate()
                link = nil
            }
        }

        private func showLatest() {
            let latest = source?.latest
            if case .some(let value) = shown, value == latest { return }
            shown = .some(latest)
            show(latest)
        }
    }

    /// The display link's target, which a generic class can't be.
    private final class FrameTicker: NSObject {
        private let tick: @MainActor () -> Void

        init(tick: @escaping @MainActor () -> Void) {
            self.tick = tick
        }

        @MainActor
        @objc
        func frame(_: CADisplayLink) {
            tick()
        }
    }
#endif
