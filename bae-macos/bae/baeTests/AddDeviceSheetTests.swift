import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// `AddDeviceSheet` takes its `result` as a `@Binding` because the presenter
/// starts the pairing off-main *after* the sheet is already up: the
/// sheet must re-render when that write lands rather than show the loading
/// snapshot it opened with. Once it shows the code it follows the pairing's
/// status, and says so in place of the code once a new device is added.
/// These drive the real sheet and check its view tree when either lands:
/// the loading spinner gives way to the code, which gives way to the added
/// device. What each says is the add-device shots'.
@Suite("AddDeviceSheet binding")
struct AddDeviceSheetTests {
    private static let adding = BridgeAddDevice(
        id: "6f1f0ab0-0000-4000-8000-000000000001",
        code: "PAIR 1234 5678",
        qrText: "PAIR-1234-5678"
    )

    @MainActor
    @Test("the sheet re-renders when its result binding becomes .success")
    func sheetUpdatesWhenResultBindingResolves() async throws {
        let holder = AddDeviceResultHolder()
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder)
                .environment(MembershipStore(handle: PairingHandle()))
                .environment(Sync()),
            size: size
        ) { _, host in
            // Loading state (binding is nil).
            try await SnapshotTestSupport.settle(host)
            #expect(count(NSProgressIndicator.self, in: host) == 1)

            // The presenter's off-main write lands: the binding resolves to a
            // code, which the sheet shows in place of the spinner.
            holder.result = .success(Self.adding)
            try await Wait.untilLaidOut(host) {
                count(NSProgressIndicator.self, in: host) == 0
            }
        }
    }

    @MainActor
    @Test("the sheet replaces the code once the person's new device is added")
    func sheetSaysWhenTheNewDeviceIsAdded() async throws {
        let holder = AddDeviceResultHolder()
        holder.result = .success(Self.adding)
        let handle = PairingHandle()
        let store = MembershipStore(handle: handle)
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder).environment(store)
                .environment(Sync()),
            size: size
        ) { _, host in
            try await Wait.until { handle.statuses != nil }
            // The sheet follows the pairing its code came with.
            #expect(handle.pairing == Self.adding.id)
            let statuses = try #require(handle.statuses)
            statuses.deliver(.waiting(request: nil))
            try await SnapshotTestSupport.settle(host)
            #expect(store.addDeviceStatus?.face == .code)

            statuses.deliver(.ended(end: .added(name: "New Phone")))
            try await Wait.until {
                store.addDeviceStatus?.face == .added(name: "New Phone")
            }
        }
        // Closing the sheet ends the subscription.
        try await Wait.until { handle.cancelled }
    }

    @Test("a declined pairing closes the sheet; the others say what happened")
    func faceOfAnEndWithoutDevice() {
        #expect(BridgeAddDeviceStatus.ended(end: .declined).face == .declined)
        #expect(BridgeAddDeviceStatus.ended(end: .expired).face == .expired)
        #expect(BridgeAddDeviceStatus.ended(end: .refused).face == .refused)
        #expect(
            BridgeAddDeviceStatus.approved(deviceName: "Tablet").face
                == .adding(name: "Tablet")
        )
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

/// A handle-less `AppHandle` that keeps the pairing followed and the
/// subscription it handed out, so a test delivers what the live read would,
/// and says when the subscription ends.
private final class PairingHandle: AppHandle, @unchecked Sendable {
    private(set) var pairing: String?
    private(set) var statuses: PairingStatuses?
    var cancelled: Bool { statuses?.cancelled ?? false }

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a fake handle has no core object")
    }

    override func followAddDevice(
        pairing: String
    ) throws -> AddDeviceStatusSubscription {
        self.pairing = pairing
        let statuses = PairingStatuses()
        self.statuses = statuses
        Wait.happened()
        return statuses
    }
}

/// The pairing statuses a test hands the sheet, one value per pull, which
/// says whether it was cancelled.
private final class PairingStatuses: AddDeviceStatusSubscription,
    @unchecked Sendable
{
    private let lock = NSLock()
    private var waiting: [BridgeAddDeviceStatus] = []
    private var puller: CheckedContinuation<BridgeAddDeviceStatus, any Error>?
    private var wasCancelled = false

    var cancelled: Bool { lock.withLock { wasCancelled } }

    init() {
        super.init(noHandle: AddDeviceStatusSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a fake subscription has no core object")
    }

    func deliver(_ status: BridgeAddDeviceStatus) {
        let puller = lock.withLock {
            let puller = self.puller
            self.puller = nil
            if puller == nil { waiting.append(status) }
            return puller
        }
        puller?.resume(returning: status)
    }

    override func next() async throws -> BridgeAddDeviceStatus {
        try await withCheckedThrowingContinuation { continuation in
            let ready: BridgeAddDeviceStatus? = lock.withLock {
                if waiting.isEmpty {
                    puller = continuation
                    return nil
                }
                return waiting.removeFirst()
            }
            if let status = ready {
                continuation.resume(returning: status)
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
