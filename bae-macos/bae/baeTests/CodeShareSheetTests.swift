import AppKit
import SwiftUI
import Testing

@testable import bae

/// `CodeShareSheet` takes its `result` as a `@Binding` because the presenter
/// generates the pairing code off-main *after* the sheet is already up: the
/// sheet must re-render when that write lands rather than show the loading
/// snapshot it opened with. This drives the real sheet through a state-backed
/// binding, flipping `nil` (loading) → `.success(code)` while it stays mounted,
/// and asserts the rendered pixels change — i.e. the body reacts to the binding.
@Suite("CodeShareSheet binding")
struct CodeShareSheetTests {
    @MainActor
    @Test("the sheet re-renders when its result binding becomes .success")
    func sheetUpdatesWhenResultBindingResolves() async throws {
        let holder = CodeShareResultHolder()
        let size = NSSize(width: 400, height: 420)
        try await SnapshotTestSupport.withHostedWindow(
            CodeShareHarness(holder: holder),
            size: size
        ) { _, host in

            // Loading state (binding is nil).
            let loading = try await SnapshotTestSupport.capturePNG(
                host,
                size: size
            )

            // The presenter's off-main write lands: the binding resolves to a
            // code, and the sheet draws something else.
            holder.result = .success("PAIR-1234-5678")
            try await Wait.until {
                try await SnapshotTestSupport.capturePNG(host, size: size)
                    != loading
            }
        }
    }
}

/// Owns the `result` the sheet binds to, so flipping it propagates through
/// SwiftUI to the live `CodeShareSheet` the same way the presenter's off-main
/// write does.
@Observable
@MainActor
private final class CodeShareResultHolder {
    var result: Result<String, Error>?
}

private struct CodeShareHarness: View {
    @Bindable
    var holder: CodeShareResultHolder

    var body: some View {
        CodeShareSheet(result: $holder.result, onDismiss: {})
    }
}
