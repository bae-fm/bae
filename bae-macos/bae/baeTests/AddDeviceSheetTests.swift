import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// `AddDeviceSheet` takes its `result` as a `@Binding` because the presenter
/// generates the code off-main *after* the sheet is already up: the
/// sheet must re-render when that write lands rather than show the loading
/// snapshot it opened with. Once it shows the code it runs the watch that
/// came with it, and says so in place of the code once a new device joins.
/// These drive the real sheet and check its view tree when either lands:
/// the loading spinner gives way to the code's scroll view, which gives way
/// to the joined device. What each says is the `add-device-code` and
/// `add-device-added` shots'.
@Suite("AddDeviceSheet binding")
struct AddDeviceSheetTests {
    private static let adding = BridgeAddDevice(
        share: BridgeRestoreCodeShare(
            code: "PAIR-1234-5678",
            qrText: "PAIR-1234-5678",
            qrCarriesS3Key: false
        ),
        watch: BridgeAddedDeviceWatch(
            noHandle: BridgeAddedDeviceWatch.NoHandle()
        )
    )

    @MainActor
    @Test("the sheet re-renders when its result binding becomes .success")
    func sheetUpdatesWhenResultBindingResolves() async throws {
        let holder = AddDeviceResultHolder()
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder)
                .environment(MembershipStore(handle: AddedDeviceHandle())),
            size: size
        ) { _, host in
            // Loading state (binding is nil).
            try await SnapshotTestSupport.settle(host)
            #expect(count(NSProgressIndicator.self, in: host) == 1)
            #expect(count(NSScrollView.self, in: host) == 0)

            // The presenter's off-main write lands: the binding resolves to a
            // code, which the sheet shows in place of the spinner.
            holder.result = .success(Self.adding)
            try await Wait.untilLaidOut(host) {
                count(NSProgressIndicator.self, in: host) == 0
                    && count(NSScrollView.self, in: host) == 1
            }
        }
    }

    @MainActor
    @Test("the sheet replaces the code once the person's new device joins")
    func sheetSaysWhenTheNewDeviceJoins() async throws {
        let holder = AddDeviceResultHolder()
        holder.result = .success(Self.adding)
        let handle = AddedDeviceHandle()
        let store = MembershipStore(handle: handle)
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder).environment(store),
            size: size
        ) { _, host in
            try await Wait.until { handle.devices != nil }
            // The sheet runs the watch that came with its code.
            #expect(handle.watch === Self.adding.watch)
            let devices = try #require(handle.devices)
            // The live read's first value: no device has joined yet.
            devices.deliver(nil)
            try await SnapshotTestSupport.settle(host)
            #expect(count(NSScrollView.self, in: host) == 1)

            devices.deliver(
                BridgeActiveDevice(
                    id: 7,
                    name: "New Phone",
                    isSelf: false,
                    canRemove: true
                )
            )
            try await Wait.until { store.addedDevice?.name == "New Phone" }
            // The joined device takes the code's place.
            try await Wait.untilLaidOut(host) {
                count(NSScrollView.self, in: host) == 0
            }
        }
        // Closing the sheet ends the watch.
        try await Wait.until { handle.cancelled }
    }

    /// How many views of `kind` the hosted sheet holds.
    @MainActor
    private func count(_ kind: NSView.Type, in host: NSView) -> Int {
        SnapshotTestSupport.descendants(of: host)
            .filter { $0.isKind(of: kind) }
            .count
    }
}

/// Owns the `result` the sheet binds to, so flipping it propagates through
/// SwiftUI to the live `AddDeviceSheet` the same way the presenter's off-main
/// write does.
@Observable
@MainActor
private final class AddDeviceResultHolder {
    var result: Result<BridgeAddDevice, Error>?
}

private struct AddDeviceHarness: View {
    @Bindable
    var holder: AddDeviceResultHolder

    var body: some View {
        AddDeviceSheet(result: $holder.result, onDismiss: {})
    }
}

/// A handle-less `AppHandle` that keeps the added-device watch and the
/// subscription it handed out, so a test delivers what the live read would,
/// and says when the watch ends.
private final class AddedDeviceHandle: AppHandle, @unchecked Sendable {
    private(set) var watch: BridgeAddedDeviceWatch?
    private(set) var devices: AddedDevices?
    var cancelled: Bool { devices?.cancelled ?? false }

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a fake handle has no core object")
    }

    override func subscribeAddedDevice(
        watch: BridgeAddedDeviceWatch
    ) -> AddedDeviceSubscription {
        self.watch = watch
        let devices = AddedDevices()
        self.devices = devices
        Wait.happened()
        return devices
    }
}

/// The added device a test hands the sheet, one value per pull, which says
/// whether it was cancelled.
private final class AddedDevices: AddedDeviceSubscription, @unchecked Sendable {
    private let lock = NSLock()
    private var waiting: [BridgeActiveDevice?] = []
    private var puller: CheckedContinuation<BridgeActiveDevice?, any Error>?
    private var wasCancelled = false

    var cancelled: Bool { lock.withLock { wasCancelled } }

    init() {
        super.init(noHandle: AddedDeviceSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a fake subscription has no core object")
    }

    func deliver(_ device: BridgeActiveDevice?) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { waiting.append(device) }
            return puller
        }
        puller?.resume(returning: device)
    }

    override func next() async throws -> BridgeActiveDevice? {
        try await withCheckedThrowingContinuation { continuation in
            let ready: BridgeActiveDevice?? = lock.withLock {
                if waiting.isEmpty {
                    puller = continuation
                    return .none
                }
                return .some(waiting.removeFirst())
            }
            if case .some(let device) = ready {
                continuation.resume(returning: device)
            }
        }
    }

    override func cancel() async throws {
        let puller = lock.recording {
            wasCancelled = true
            let puller = self.puller
            self.puller = nil
            return puller
        }
        puller?.resume(throwing: BridgeError.Cancelled)
    }
}
