import Foundation
import Observation
import os

#if os(macOS)
    import AppKit
#endif

private let logger = Logger.bae("LatestValues")

// The one place the app touches the bridge's `KeyedValueStream`: values core
// sends many times a second (transfer bytes and rates, file shares, export
// percents, playback and preview positions, import percents, a library
// open's steps) arrive in a stream the bridge holds and writes itself, and the
// only reader of a value is `FrameFollowing`, which hands it to the native
// view drawing that key once per display frame. App code passes streams and
// sources around; it never holds a value, so nothing SwiftUI observes can
// change with one. Which keys started or stopped holding a value is pulled
// through `ScreenSubscription`, as every screen value is.
// `scripts/keyed-value-stream-boundary.py` keeps every other file off the
// bridge's stream.

// The bridge's stream is pulled like every other bridge pull object.
extension KeyedValueStream: PulledSubscription {}

/// The latest value of each key of one fast stream, for the native views
/// that draw a key. Until it is fed it holds nothing, and a preview's holds
/// fixed values that nothing feeds.
public final class LatestStream<Value: Sendable & Equatable>: Sendable {
    private enum Backing {
        case unfed
        case preview([Data: Value])
        case fed(KeyedValueStream)
    }

    /// A value the app shows in place of a key's own, while each value of
    /// the key's own leaves it standing.
    private struct Hold: Sendable {
        let value: Value
        let stands: @Sendable (Value) -> Bool
    }

    fileprivate let activity = KeyActivity()
    private let read: @Sendable (BridgeFastValue) -> Value?
    private let backing: OSAllocatedUnfairLock<Backing>
    private let holds = OSAllocatedUnfairLock<[Data: Hold]>(
        initialState: [:]
    )

    private init(
        holding values: [Data: Value],
        written: Bool = false,
        read: @escaping @Sendable (BridgeFastValue) -> Value?
    ) {
        self.read = read
        backing = OSAllocatedUnfairLock(
            uncheckedState: values.isEmpty && !written
                ? .unfed : .preview(values)
        )
    }

    /// One key of the stream, for the native view that draws it.
    public func source(for key: Data) -> LatestSource<Value> {
        LatestSource(stream: self, key: key)
    }

    /// The key a file's, release's or track's id names, as its UTF-8 bytes.
    public func source(for key: String) -> LatestSource<Value> {
        source(for: Data(key.utf8))
    }

    /// Values a preview holds under text ids, under each id's UTF-8 bytes.
    fileprivate static func keyed(_ values: [String: Value]) -> [Data: Value] {
        Dictionary(
            uniqueKeysWithValues: values.map { (Data($0.key.utf8), $0.value) }
        )
    }

    /// The key's latest value: one the app holds in front of it while it
    /// stands, else the key's own.
    fileprivate func latest(for key: Data) -> Value? {
        let own = own(for: key)
        return holds.withLock { holds in
            guard let hold = holds[key] else { return own }
            if let own, hold.stands(own) { return hold.value }
            holds[key] = nil
            return own
        }
    }

    /// Show `value` for `key` in place of its own, until one of its own
    /// values does not leave it `standing`; a key holding none drops it.
    fileprivate func hold(
        _ value: Value,
        for key: Data,
        while standing: @escaping @Sendable (Value) -> Bool
    ) {
        holds.withLock { $0[key] = Hold(value: value, stands: standing) }
    }

    /// What the key's feed, or the preview, holds for it.
    fileprivate func own(for key: Data) -> Value? {
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
    /// cancelled or dropped, pulling which keys started or stopped holding a
    /// value for the views watching them. A feed with failed reads hands each
    /// to `onFailedRead`; a pulled failure of any other feed is a bug.
    fileprivate func fed(
        onFailedRead: (@MainActor @Sendable (any Error) -> Void)? = nil,
        _ open: () -> KeyedValueStream
    ) -> LatestFeed {
        let stream = open()
        backing.withLockUnchecked { backing in
            guard case .unfed = backing else {
                preconditionFailure("a \(Value.self) stream is fed twice")
            }
            backing = .fed(stream)
        }
        let activity = activity
        let streamName = "\(Value.self)"
        return LatestFeed(
            stream: stream,
            pulls: ScreenSubscription(
                stream,
                onValue: { keys in activity.keysChanged(keys) },
                onError: onFailedRead ?? { error in
                    reportStreamBug(error, stream: streamName)
                }
            ),
            activity: activity
        )
    }
}

#if DEBUG
    /// A test's writing of a stream that nothing feeds, in place of core: a
    /// key starting or stopping is told, a value moving is read on the next
    /// frame, as from a fed stream.
    public final class LatestTestWriter<Value: Sendable & Equatable>: Sendable {
        public let stream: LatestStream<Value>

        fileprivate init(stream: LatestStream<Value>) {
            self.stream = stream
        }

        /// Set `key`'s value; a stream that holds one value holds it under
        /// `""`.
        public func set(_ value: Value, for key: String = "") {
            set(value, for: Data(key.utf8))
        }

        public func set(_ value: Value, for key: Data) {
            stream.write(key) { $0[key] = value }
        }

        public func remove(_ key: String = "") {
            let key = Data(key.utf8)
            stream.write(key) { $0.removeValue(forKey: key) }
        }
    }

    extension LatestStream {
        /// A stream a test writes, through the writer it hands back.
        public static func written() -> LatestTestWriter<Value> {
            LatestTestWriter(
                stream: LatestStream(holding: [:], written: true) { _ in nil }
            )
        }

        fileprivate func write(
            _ key: Data,
            _ change: (inout [Data: Value]) -> Void
        ) {
            let started = backing.withLockUnchecked { backing in
                guard case .preview(var values) = backing else {
                    preconditionFailure("only a written stream is written")
                }
                let held = values[key] != nil
                change(&values)
                backing = .preview(values)
                return held != (values[key] != nil)
            }
            if started {
                let activity = activity
                Task { @MainActor in activity.keysChanged([key]) }
            }
        }
    }
#endif

/// A fed stream's feed, which stops when it is cancelled or dropped; the
/// stream then holds nothing, and every view watching one of its keys is
/// told, so it reads that the key holds none.
public final class LatestFeed: Sendable {
    private let stream: KeyedValueStream
    private let pulls: ScreenSubscription<KeyedValueStream>
    private let activity: KeyActivity

    fileprivate init(
        stream: KeyedValueStream,
        pulls: ScreenSubscription<KeyedValueStream>,
        activity: KeyActivity
    ) {
        self.stream = stream
        self.pulls = pulls
        self.activity = activity
    }

    public func cancel() {
        end()
    }

    deinit {
        end()
    }

    private func end() {
        stream.cancel()
        pulls.stop()
        let activity = activity
        Task { @MainActor in activity.everyKeyChanged() }
    }
}

/// A pulled failure of a stream whose feed has no failed reads: the bridge
/// broke its own guarantee, so it is reported to the crash reporter core
/// reports its bugs to. The pulls go on.
private func reportStreamBug(_ error: any Error, stream: String) {
    let detail = "the \(stream) stream answered \(String(reflecting: error))"
    logger.error("\(detail)")
    CoreBugReporter().report(operation: "KeyedValueStream.next", detail: detail)
}

/// The key a stream that holds one value holds it under.
private let onlyKey = Data()

extension LatestStream where Value == BridgeReleaseUploadTransfer {
    /// Each release's upload transfer, by release id; a preview's holds
    /// `values` and nothing feeds it.
    public static func releaseUploads(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
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
        fed(onFailedRead: onError) {
            appHandle.feedReleaseUploadTransfers()
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

    /// Fill from core, telling `onError` each failed read.
    public func feed(
        from appHandle: AppHandle,
        onError: @escaping @MainActor @Sendable (any Error) -> Void
    ) -> LatestFeed {
        fed(onFailedRead: onError) { appHandle.feedUploadTotalTransfer() }
    }
}

extension LatestStream where Value == Double {
    /// The share of each file in flight its object has moved, by file id; a
    /// preview's holds `values` and nothing feeds it.
    public static func fileShares(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
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
        fed { appHandle.feedUploadFileShares() }
    }

    /// Fill with the files being downloaded to keep whole.
    public func feedPins(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedPinFileShares() }
    }
}

extension LatestStream where Value == BridgeDownloadTransferProgress {
    /// Each downloading release's bytes and rate, by release id; a
    /// preview's holds `values` and nothing feeds it.
    public static func downloads(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
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
        fed { appHandle.feedDownloadTransfers() }
    }
}

extension LatestStream where Value == BridgeMakeLocalProgress {
    /// How far each release being made local has downloaded, by release id;
    /// a preview's holds `values` and nothing feeds it.
    public static func makeLocals(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
            if case .makeLocal(let progress) = $0 {
                progress
            }
            else {
                nil
            }
        }
    }

    #if os(macOS)
        /// Fill from core.
        public func feed(from appHandle: AppHandle) -> LatestFeed {
            fed { appHandle.feedMakeLocalTransfers() }
        }
    #endif
}

extension LatestStream where Value == UInt8 {
    /// Each exporting release's percent, by release id; a preview's holds
    /// `values` and nothing feeds it.
    public static func exportPercents(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
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
            fed { appHandle.feedExportPercents() }
        }
    #endif
}

extension LatestStream where Value == UInt32 {
    /// How far through its step each running import is, by candidate key;
    /// a preview's holds `values` and nothing feeds it.
    public static func importPercents(holding values: [Data: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: values) {
            if case .importPercent(let percent) = $0 {
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
            fed { appHandle.feedImportPercents() }
        }
    #endif
}

#if os(macOS)
    extension LatestStream where Value == BridgeSelectionActionProgress {
        /// How far a bulk action over an import list's selection has got,
        /// while it runs; a preview's holds `value` and nothing feeds it.
        public static func bulkActionProgress(holding value: Value? = nil)
            -> LatestStream
        {
            LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
                if case .selectionActionProgress(let progress) = $0 {
                    progress
                }
                else {
                    nil
                }
            }
        }

        /// Fill with the next bulk action over `list`'s selection.
        public func feed(from list: any ImportListSubscriptionProtocol)
            -> LatestFeed
        {
            fed { list.selectionActionProgress() }
        }
    }
#endif

extension LatestStream where Value == BridgeOpenProgress {
    /// How far an open of a library has come, while it runs a step; a
    /// preview's holds `value` and nothing feeds it.
    public static func openProgress(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .openProgress(let progress) = $0 {
                progress
            }
            else {
                nil
            }
        }
    }

    /// Fill with the steps of the open `opening` is handed to.
    public func feed(from opening: BridgeLibraryOpening) -> LatestFeed {
        fed { opening.feedOpenSteps() }
    }
}

extension LatestStream where Value == BridgeSetupProgress {
    /// How far a setup call has come, while it runs a step; a preview's
    /// holds `value` and nothing feeds it.
    public static func setupProgress(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .setupProgress(let progress) = $0 {
                progress
            }
            else {
                nil
            }
        }
    }

    /// Fill with the steps of the call `setup` is handed to.
    public func feed(from setup: BridgeLibrarySetup) -> LatestFeed {
        fed { setup.feedSetupSteps() }
    }
}

extension LatestStream where Value == BridgeBootstrapStep {
    /// The counted step a restore, join or reset runs, while it runs one; a
    /// preview's holds `value` and nothing feeds it.
    public static func bootstrapSteps(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .bootstrapStep(let step) = $0 {
                step
            }
            else {
                nil
            }
        }
    }

    /// Fill with the counted steps of the restore, join or reset `setup` is
    /// handed to.
    public func feed(from setup: BridgeLibrarySetup) -> LatestFeed {
        fed { setup.feedSetupSteps() }
    }
}

extension LatestStream where Value == BridgeStorageSetupProgress {
    /// How far a storage setup has come, while it runs a step; a preview's
    /// holds `value` and nothing feeds it.
    public static func storageSetupProgress(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .storageSetupProgress(let progress) = $0 {
                progress
            }
            else {
                nil
            }
        }
    }

    /// Fill with the steps of the storage setup `setup` is handed to.
    public func feed(from setup: BridgeLibrarySetup) -> LatestFeed {
        fed { setup.feedSetupSteps() }
    }
}

extension LatestStream where Value == BridgeClosingStep {
    /// The step a library's close runs, while it runs one; a preview's holds
    /// `value` and nothing feeds it.
    public static func closingSteps(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .closingStep(let step) = $0 {
                step
            }
            else {
                nil
            }
        }
    }

    /// Fill with the steps of the close `closing` is handed to.
    public func feed(from closing: BridgeLibraryClosing) -> LatestFeed {
        fed { closing.feedClosingSteps() }
    }
}

#if os(macOS)
    extension LatestStream where Value == BridgeCatalogApplyStep {
        /// The network read an apply of a catalog release waits on, while it
        /// waits on one; a preview's holds `value` and nothing feeds it.
        public static func catalogApplySteps(holding value: Value? = nil)
            -> LatestStream
        {
            LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
                if case .catalogApplyStep(let step) = $0 {
                    step
                }
                else {
                    nil
                }
            }
        }

        /// Fill with the steps of the apply `apply` is handed to.
        public func feed(from apply: BridgeCatalogApply) -> LatestFeed {
            fed { apply.feedApplySteps() }
        }
    }
#endif

extension LatestStream {
    /// The value of a stream that holds one, for the native view that draws
    /// it.
    public var only: LatestSource<Value> { source(for: onlyKey) }
}

extension LatestStream where Value == BridgeTrackPosition {
    /// Where the playing track is, under its track id; a preview's holds
    /// `values` and nothing feeds it.
    public static func trackPositions(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
            if case .trackPosition(let position) = $0 {
                position
            }
            else {
                nil
            }
        }
    }

    /// Fill from core.
    public func feed(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedPlaybackPositions() }
    }

    /// Show `key` at `progress` of its length, as the seek the person just
    /// made there, until a position of a later applied seek arrives, for
    /// every view drawing that track. Nothing while the key holds no
    /// position of a known length.
    fileprivate func holdSeek(_ key: Data, at progress: Double) {
        guard let current = own(for: key),
            let durationMs = current.durationMs
        else {
            return
        }
        let progress = max(0, min(1, progress))
        let seekRevision = current.seekRevision
        hold(
            BridgeTrackPosition(
                positionMs: Int64(progress * Double(durationMs)),
                durationMs: durationMs,
                progress: progress,
                seekRevision: seekRevision
            ),
            for: key
        ) { $0.seekRevision == seekRevision }
    }
}

extension LatestSource where Value == BridgeTrackPosition {
    /// Show this track at `progress` of its length, as the seek the person
    /// just made, until core has applied it.
    public func holdSeek(at progress: Double) {
        stream.holdSeek(key, at: progress)
    }
}

extension LatestStream where Value == BridgeTrackBuffered {
    /// The parts of the playing track whose bytes this device holds, under
    /// its track id; a preview's holds `values` and nothing feeds it.
    public static func trackBuffered(holding values: [String: Value] = [:])
        -> LatestStream
    {
        LatestStream(holding: keyed(values)) {
            if case .trackBuffered(let buffered) = $0 {
                buffered
            }
            else {
                nil
            }
        }
    }

    /// Fill from core.
    public func feed(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedTrackBuffered() }
    }
}

extension LatestStream where Value == Bool {
    /// True under the playing track's id while its load waits on storage;
    /// a preview's holds true under each of `waiting` and nothing feeds it.
    public static func trackStorageWaits(waiting: Set<String> = [])
        -> LatestStream
    {
        LatestStream(
            holding: keyed(
                Dictionary(uniqueKeysWithValues: waiting.map { ($0, true) })
            )
        ) {
            if case .trackWaitsOnStorage = $0 {
                true
            }
            else {
                nil
            }
        }
    }

    /// Fill from core.
    public func feed(from appHandle: AppHandle) -> LatestFeed {
        fed { appHandle.feedTrackStorageWaits() }
    }
}

extension LatestStream where Value == BridgePreviewPosition {
    /// Where the preview is in its clip, while one is loaded; a preview's
    /// holds `value` and nothing feeds it.
    public static func previewPosition(holding value: Value? = nil)
        -> LatestStream
    {
        LatestStream(holding: value.map { [onlyKey: $0] } ?? [:]) {
            if case .previewPosition(let position) = $0 {
                position
            }
            else {
                nil
            }
        }
    }

    #if os(macOS)
        /// Fill from core.
        public func feed(from appHandle: AppHandle) -> LatestFeed {
            fed { appHandle.feedPreviewPosition() }
        }
    #endif
}

/// One key of a fast stream, which a view hands to the native view that
/// draws it. Only `FrameFollowing` reads its value.
public struct LatestSource<Value: Sendable & Equatable>: Sendable {
    fileprivate let stream: LatestStream<Value>
    fileprivate let key: Data

    fileprivate var latest: Value? { stream.latest(for: key) }

    /// Whether `other` names the same key of the same stream.
    fileprivate func isSame(as other: Self) -> Bool {
        stream === other.stream && key == other.key
    }
}

/// Which keys started or stopped holding a value, told to the native views
/// watching each on the main actor. A value moving tells nothing.
private final class KeyActivity: Sendable {
    struct Watch: Hashable, Sendable {
        let key: Data
        let id: Int
    }

    private struct Watchers {
        var byKey: [Data: [Int: @MainActor @Sendable () -> Void]] = [:]
        var nextId = 0
    }

    private let watchers = OSAllocatedUnfairLock(initialState: Watchers())

    func watch(
        _ key: Data,
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

    /// Tell the views watching `keys` that each started or stopped.
    @MainActor
    func keysChanged(_ keys: [Data]) {
        let watching = watchers.withLock { watchers in
            keys.flatMap { (watchers.byKey[$0] ?? [:]).values }
        }
        for changed in watching {
            changed()
        }
    }

    /// Tell every watching view to read its key again: the stream ended.
    @MainActor
    func everyKeyChanged() {
        let watching = watchers.withLock { watchers in
            watchers.byKey.values.flatMap(\.values)
        }
        for changed in watching {
            changed()
        }
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
            self?.frame()
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

        /// One display frame, as the display link runs it: show the key's
        /// latest value if it changed. A view runs one itself to show at once
        /// what it just did to the key, such as a seek; a test's hosted
        /// window is past every display, where no link runs, so the test
        /// runs its frames itself.
        public func frame() {
            showLatest()
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
