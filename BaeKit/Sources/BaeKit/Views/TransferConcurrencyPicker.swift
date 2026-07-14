import SwiftUI

/// A segmented 1...8 picker for a blob-transfer concurrency setting —
/// simultaneous uploads or downloads. Reads the current value from the config
/// mirror and writes it through `setValue`; a failed write (an out-of-range
/// value, a config-write error) is surfaced and the picker keeps showing the
/// stored value. It holds no optimistic state: the selection binding reads the
/// mirror, which only changes once the config invalidation from a successful
/// write lands, so a rejected change snaps back on its own.
@MainActor
public struct TransferConcurrencyPicker: View {
    public let title: LocalizedStringKey
    public let value: UInt32
    public let setValue: @Sendable (UInt32) throws -> Void
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// is core's answer, and the sink is the one place that drops it.
    public let showError: @MainActor (any Error) -> Void

    public init(
        title: LocalizedStringKey,
        value: UInt32,
        setValue: @escaping @Sendable (UInt32) throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.title = title
        self.value = value
        self.setValue = setValue
        self.showError = showError
    }

    /// The range the setters accept (bae-core's `MAX_CONCURRENT_TRANSFERS`);
    /// the bridge carries the value, not the bound, so the UI states it.
    private static let choices: [UInt32] = Array(1...8)

    public var body: some View {
        Picker(title, selection: binding) {
            ForEach(Self.choices, id: \.self) { n in
                Text(n.formatted()).tag(n)
            }
        }
        .pickerStyle(.segmented)
    }

    private var binding: Binding<UInt32> {
        Binding(
            get: { value },
            set: { n in
                do {
                    try setValue(n)
                }
                catch {
                    showError(error)
                }
            }
        )
    }
}
