import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// `AddDeviceSheet` takes its `result` as a `@Binding` because the presenter
/// generates the code off-main *after* the sheet is already up: the
/// sheet must re-render when that write lands rather than show the loading
/// snapshot it opened with. It also watches the person's devices while it
/// shows, and says so in place of the code once a new one joins. These drive
/// the real sheet and assert the rendered pixels change when either lands.
@Suite("AddDeviceSheet binding")
struct AddDeviceSheetTests {
    private static let share = BridgeRestoreCodeShare(
        code: "PAIR-1234-5678",
        qrText: "PAIR-1234-5678",
        qrCarriesS3Key: false
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
            let loading = try await SnapshotTestSupport.capturePNG(
                host,
                size: size
            )

            // The presenter's off-main write lands: the binding resolves to a
            // code, and the sheet draws something else.
            holder.result = .success(Self.share)
            try await Wait.until {
                try await SnapshotTestSupport.capturePNG(host, size: size)
                    != loading
            }
        }
    }

    @MainActor
    @Test("the sheet replaces the code once the person's new device joins")
    func sheetSaysWhenTheNewDeviceJoins() async throws {
        let holder = AddDeviceResultHolder()
        holder.result = .success(Self.share)
        let handle = AddedDeviceHandle()
        let store = MembershipStore(handle: handle)
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder).environment(store),
            size: size
        ) { _, host in
            try await Wait.until { handle.callback != nil }
            let callback = try #require(handle.callback)
            // The live read's first value: no device has joined yet.
            callback.onValue(device: nil)
            let code = try await SnapshotTestSupport.capturePNG(
                host,
                size: size
            )

            callback.onValue(
                device: BridgeActiveDevice(
                    id: 7,
                    name: "New Phone",
                    isSelf: false,
                    canRemove: true
                )
            )
            try await Wait.until { store.addedDevice?.name == "New Phone" }
            try await Wait.until {
                try await SnapshotTestSupport.capturePNG(host, size: size)
                    != code
            }
        }
        // Closing the sheet ends the watch.
        try await Wait.until { handle.cancelled }
    }
}

/// Owns the `result` the sheet binds to, so flipping it propagates through
/// SwiftUI to the live `AddDeviceSheet` the same way the presenter's off-main
/// write does.
@Observable
@MainActor
private final class AddDeviceResultHolder {
    var result: Result<BridgeRestoreCodeShare, Error>?
}

private struct AddDeviceHarness: View {
    @Bindable
    var holder: AddDeviceResultHolder

    var body: some View {
        AddDeviceSheet(result: $holder.result, onDismiss: {})
    }
}

/// A handle-less `AppHandle` that keeps the added-device watch's callback, so
/// a test delivers what the live read would, and says when the watch ends.
private final class AddedDeviceHandle: AppHandle, @unchecked Sendable {
    private(set) var callback: (any AddedDeviceCallback)?
    private(set) var cancelled = false

    init() {
        super.init(noHandle: AppHandle.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a fake handle has no core object")
    }

    override func subscribeAddedDevice(
        callback: any AddedDeviceCallback
    ) -> LiveSubscription {
        self.callback = callback
        return CancelRecordingSubscription { [weak self] in
            self?.cancelled = true
        }
    }
}

/// A handle-less subscription that reports its cancel.
private final class CancelRecordingSubscription: LiveSubscription,
    @unchecked Sendable
{
    private let onCancel: () -> Void

    init(onCancel: @escaping () -> Void) {
        self.onCancel = onCancel
        super.init(noHandle: LiveSubscription.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        preconditionFailure("a recording subscription has no core object")
    }

    override func cancel() { onCancel() }
}
