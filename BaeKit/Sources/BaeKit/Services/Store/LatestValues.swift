import Foundation
import os

#if os(macOS)
    import AppKit
#endif

// The one place the app touches the bridge's `KeyedValueStream`: values core
// sends many times a second (transfer bytes and rates, file shares, export
// percents) arrive in a stream the bridge holds and writes itself, and the
// only reader of a value is `FrameFollowing`, which hands it to the native
// view drawing that key once per display frame. App code passes streams and
// sources around; it never holds a value, so nothing SwiftUI observes can
// change with one. `scripts/keyed-value-stream-boundary.py` keeps every other
// file off the bridge's stream.

/// The latest value of each key of one fast stream, for the native views
/// that draw a key. Until it is fed it holds nothing, and a preview's holds
/// fixed values that nothing feeds.
public final class LatestStream<Value: Sendable & Equatable>: Sendable {
    private enum Backing {
        case unfed
        case preview([String: Value])
        case fed(KeyedValueStream)
    }

    fileprivate let activity = KeyActivity()
    private let read: @Sendable (BridgeFastValue) -> Value?
    private let backing: OSAllocatedUnfairLock<Backing>

    private init(
        holding values: [String: Value],
        read: @escaping @Sendable (BridgeFastValue) -> Value?
    ) {
        self.read = read
        backing = OSAllocatedUnfairLock(
            uncheckedState: values.isEmpty ? .unfed : .preview(values)
        )
    }

    /// One key of the stream, for the native view that draws it.
    public func source(for key: String) -> LatestSource<Value> {
        LatestSource(stream: self, key: key)
    }

    fileprivate func latest(for key: String) -> Value? {
        switch backing.withLockUnchecked({ $0 }) {
        case .unfed:
            return nil
        case .preview(let values):
            return values[key]
        case .fed(let stream):
            return stream.latest(key: key)
                .map { value in
                    guard let read = read(value) else {
                        preconditionFailure(
                            "a \(Value.self) stream was fed \(value)"
                        )
                    }
                    return read
                }
        }
    }

    /// Hold what `open` starts feeding, until the returned feed is
    /// cancelled or dropped.
    fileprivate func fed(_ open: (KeyActivity) -> KeyedValueStream)
        -> LatestFeed
    {
        let stream = open(activity)
        backing.withLockUnchecked { backing in
            guard case .unfed = backing else {
                preconditionFailure("a \(Value.self) stream is fed twice")
            }
            backing = .fed(stream)
        }
        return LatestFeed(stream: stream)
    }
}

/// A fed stream's feed, which stops when it is cancelled or dropped; the
/// stream then holds nothing.
public final class LatestFeed: Sendable {
    private let stream: KeyedValueStream

    fileprivate init(stream: KeyedValueStream) {
        self.stream = stream
    }

    public func cancel() {
        stream.cancel()
    }

    deinit {
        stream.cancel()
    }
}

/// The key a stream that holds one value holds it under.
private let onlyKey = ""

extension LatestStream where Value == BridgeReleaseUploadTransfer {
    /// Each release's upload transfer, by release id; a preview's holds
    /// `values` and nothing feeds it.
    public static func releaseUploads(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: values) {
            if case .releaseUpload(let transfer) = $0 {
                transfer
            }
            else {
                nil
            }
        }
    }

    /// Fill from core, telling `onError` each failed read.
    public func feed(
        from appHandle: AppHandle,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> LatestFeed {
        fed {
            appHandle.feedReleaseUploadTransfers(
                activity: $0,
                failure: StreamFailureSink(onError)
            )
        }
    }
}

extension LatestStream where Value == BridgeUploadTransfer {
    /// The files being sent together, while any is; a preview's holds
    /// `value` and nothing feeds it.
    public static func uploadTotal(holding value: Value? = nil) -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .uploadTotal(let transfer) = $0 {
                transfer
            }
            else {
                nil
            }
        }
    }

    /// The stream's one value, for the native view that draws it.
    public var only: LatestSource<Value> { source(for: onlyKey) }

    /// Fill from core, telling `onError` each failed read.
    public func feed(
        from appHandle: AppHandle,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> LatestFeed {
        fed {
            appHandle.feedUploadTotalTransfer(
                activity: $0,
                failure: StreamFailureSink(onError)
            )
        }
    }
}

extension LatestStream where Value == Double {
    /// The share of each file in flight its object has moved, by file id; a
    /// preview's holds `values` and nothing feeds it.
    public static func fileShares(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: values) {
            if case .fileShare(let fraction) = $0 {
                fraction
            }
            else {
                nil
            }
        }
    }

    /// Fill with the files being sent.
    public func feedUploads(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedUploadFileShares(activity: $0) }
    }

    /// Fill with the files being downloaded to keep whole.
    public func feedPins(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedPinFileShares(activity: $0) }
    }
}

extension LatestStream where Value == BridgeDownloadTransferProgress {
    /// Each downloading release's bytes and rate, by release id; a
    /// preview's holds `values` and nothing feeds it.
    public static func downloads(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: values) {
            if case .download(let progress) = $0 {
                progress
            }
            else {
                nil
            }
        }
    }

    /// Fill from core.
    public func feed(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedDownloadTransfers(activity: $0) }
    }
}

extension LatestStream where Value == UInt8 {
    /// Each exporting release's percent, by release id; a preview's holds
    /// `values` and nothing feeds it.
    public static func exportPercents(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: values) {
            if case .exportPercent(let percent) = $0 {
                percent
            }
            else {
                nil
            }
        }
    }

    #if os(macOS)
        /// Fill from core.
        public func feed(from appHandle: AppHandle) -> LatestFeed {
            fed { appHandle.feedExportPercents(activity: $0) }
        }
    #endif
}

/// One key of a fast stream, which a view hands to the native view that
/// draws it. Only `FrameFollowing` reads its value.
public struct LatestSource<Value: Sendable & Equatable>: Sendable {
    fileprivate let stream: LatestStream<Value>
    fileprivate let key: String

    fileprivate var latest: Value? { stream.latest(for: key) }

    /// Whether `other` names the same key of the same stream.
    fileprivate func isSame(as other: Self) -> Bool {
        stream === other.stream && key == other.key
    }
}

/// Which keys started or stopped holding a value, told to the native views
/// watching each on the main thread. A value moving tells nothing.
private final class KeyActivity: KeyActivityCallback, Sendable {
    struct Watch: Hashable, Sendable {
        let key: String
        let id: Int
    }

    private struct Watchers {
        var byKey: [String: [Int: @MainActor @Sendable () -> Void]] = [:]
        var nextId = 0
    }

    private let watchers = OSAllocatedUnfairLock(initialState: Watchers())

    func watch(
        _ key: String,
        _ changed: @escaping @MainActor @Sendable () -> Void
    ) -> Watch {
        watchers.withLock { watchers in
            let watch = Watch(key: key, id: watchers.nextId)
            watchers.nextId += 1
            watchers.byKey[key, default: [:]][watch.id] = changed
            return watch
        }
    }

    func stopWatching(_ watch: Watch) {
        watchers.withLock { watchers in
            watchers.byKey[watch.key]?.removeValue(forKey: watch.id)
            if watchers.byKey[watch.key]?.isEmpty == true {
                watchers.byKey.removeValue(forKey: watch.key)
            }
        }
    }

    /// Hop to the main thread once for the keys that started or stopped,
    /// for the views watching them.
    func onKeysChanged(keys: [String]) {
        let watching = watchers.withLock { watchers in
            keys.flatMap { (watchers.byKey[$0] ?? [:]).values }
        }
        guard !watching.isEmpty else { return }
        Task { @MainActor in
            for changed in watching {
                changed()
            }
        }
    }
}

/// A fed stream's failed read, told on the main thread.
private final class StreamFailureSink: StreamFailureCallback, Sendable {
    private let fail: @MainActor @Sendable (any Error) -> Void

    init(_ fail: @escaping @MainActor @Sendable (any Error) -> Void) {
        self.fail = fail
    }

    func onError(error: BridgeError) {
        Task { @MainActor in fail(error) }
    }
}

#if os(macOS)
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
        private var watch: KeyActivity.Watch?
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

        /// Start reading each frame as the key starts, and stop as it goes
        /// idle, showing what it holds either way.
        private func activityChanged() {
            let active = showLatest() != nil
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
