import AppKit
import BaeKit
import SwiftUI
import Testing

@testable import bae

/// `AddDeviceSheet` takes its `result` as a `@Binding` because the presenter
/// generates the code off-main *after* the sheet is already up: the
/// sheet must re-render when that write lands rather than show the loading
/// snapshot it opened with. This drives the real sheet through a state-backed
/// binding, flipping `nil` (loading) → `.success(code)` while it stays mounted,
/// and asserts the rendered pixels change — i.e. the body reacts to the binding.
@Suite("AddDeviceSheet binding")
struct AddDeviceSheetTests {
    @MainActor
    @Test("the sheet re-renders when its result binding becomes .success")
    func sheetUpdatesWhenResultBindingResolves() async throws {
        let holder = AddDeviceResultHolder()
        let size = NSSize(width: 400, height: 480)
        try await SnapshotTestSupport.withHostedWindow(
            AddDeviceHarness(holder: holder),
            size: size
        ) { _, host in

            // Loading state (binding is nil).
            let loading = try await SnapshotTestSupport.capturePNG(
                host,
                size: size
            )

            // The presenter's off-main write lands: the binding resolves to a
            // code, and the sheet draws something else.
            holder.result = .success(
                BridgeRestoreCodeShare(
                    code: "PAIR-1234-5678",
                    qrText: "PAIR-1234-5678",
                    qrCarriesS3Key: false
                )
            )
            try await Wait.until {
                try await SnapshotTestSupport.capturePNG(host, size: size)
                    != loading
            }
        }
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
