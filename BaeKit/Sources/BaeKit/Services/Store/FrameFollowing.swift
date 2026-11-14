#if os(macOS)
    import AppKit
    import Foundation

    /// A native view's reading of one key of a fast stream on the display's
    /// own clock: while the key is moving, the view's display link reads the
    /// key's latest value each frame and shows it if it changed; while the
    /// key is at rest or holds nothing, nothing runs, and the view shows the
    /// key's value once on each pull that names the key. Values arriving faster than frames are never
    /// all shown; the screen shows the latest one each frame.
    ///
    /// A value that is itself a line drawn from the time, such as a position
    /// anchor, also takes `onFrame`, which each frame of a moving key runs
    /// with the key's latest value and the frame's time on the frame clock,
    /// whether or not the value changed.
    @MainActor
    public final class FrameFollowing<Value: Sendable & Equatable> {
        private weak var view: NSView?
        private let show: (Value?) -> Void
        private let onFrame: ((Value, UInt64) -> Void)?
        private var source: LatestSource<Value>?
        private var watch: KeyActivity.Watch?
        private var link: CADisplayLink?
        private lazy var ticker = FrameTicker { [weak self] frameNs in
            self?.frame(atNs: frameNs)
        }
        /// What was last shown; `.none` until something is.
        private var shown: Value??

        /// Follow on `view`'s display, showing each new value with `show`.
        public init(
            view: NSView,
            show: @escaping (Value?) -> Void,
            onFrame: ((Value, UInt64) -> Void)? = nil
        ) {
            self.view = view
            self.show = show
            self.onFrame = onFrame
        }

        /// Read `source` from now on; a source for the key already read
        /// changes nothing.
        public func follow(_ source: LatestSource<Value>) {
            if let current = self.source, current.isSame(as: source) { return }
            stop()
            self.source = source
            watch = source.stream.activity.watch(source.key) { [weak self] in
                self?.activityChanged()
            }
            activityChanged()
        }

        public func stop() {
            if let source, let watch {
                source.stream.activity.stopWatching(watch)
            }
            link?.invalidate()
            link = nil
            source = nil
            watch = nil
            shown = .none
        }

        /// Start reading each frame as the key starts moving, and stop as it
        /// rests or goes idle, showing what it holds either way.
        private func activityChanged() {
            showLatest()
            let active = source?.moving ?? false
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

        /// One display frame, as the display link runs it: show the key's
        /// latest value if it changed. A view runs one itself to show at once
        /// what it just did to the key, such as a seek; a test's hosted
        /// window is past every display, where no link runs, so the test
        /// runs its frames itself.
        public func frame() {
            showLatest()
        }

        /// One display frame timed at `frameNs`.
        private func frame(atNs frameNs: UInt64) {
            if let latest = showLatest(), let onFrame {
                onFrame(latest, frameNs)
            }
        }

        /// Show the key's latest value if it changed, and answer it.
        @discardableResult
        private func showLatest() -> Value? {
            let latest = source?.latest
            if case .some(let value) = shown, value == latest { return latest }
            shown = .some(latest)
            show(latest)
            return latest
        }
    }

    /// The display link's target, which a generic class can't be.
    private final class FrameTicker: NSObject {
        private let tick: @MainActor (UInt64) -> Void

        init(tick: @escaping @MainActor (UInt64) -> Void) {
            self.tick = tick
        }

        @MainActor
        @objc
        func frame(_ link: CADisplayLink) {
            // The frame is shown at its target time, which is on the frame
            // clock: the clock of `CACurrentMediaTime`.
            tick(FrameClock.ns(ofMediaTime: link.targetTimestamp))
        }
    }
#endif
