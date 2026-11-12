import SwiftUI

/// A pop-up of the counts 1 up to core's limit, for a file-transfer concurrency
/// setting: simultaneous uploads or downloads. Reads the current value from the
/// config mirror and writes it through `setValue`; a failed write (an out-of-range
/// value, a config-write error) is surfaced and the picker keeps showing the
/// stored value. It holds no optimistic state: the selection binding reads the
/// mirror, which only changes once the config subscription delivers a
/// successful write, so a rejected change snaps back on its own.
@MainActor
public struct TransferConcurrencyPicker: View {
    public let title: LocalizedStringKey
    public let value: UInt32
    /// The most core accepts (`BridgeConfig.maxConcurrentTransfers`).
    public let limit: UInt32
    public let setValue: @Sendable (UInt32) async throws -> Void
    /// Takes the error, not a rendered line: whether a failure is worth showing
    /// is core's answer, and the sink is the one place that drops it.
    public let showError: @MainActor (any Error) -> Void

    public init(
        title: LocalizedStringKey,
        value: UInt32,
        limit: UInt32,
        setValue: @escaping @Sendable (UInt32) async throws -> Void,
        showError: @escaping @MainActor (any Error) -> Void
    ) {
        self.title = title
        self.value = value
        self.limit = limit
        self.setValue = setValue
        self.showError = showError
    }

    public var body: some View {
        Picker(title, selection: binding) {
            ForEach(Array(1...limit), id: \.self) { n in
                Text(n.formatted()).tag(n)
            }
        }
        .pickerStyle(.menu)
    }

    private var binding: Binding<UInt32> {
        Binding(
            get: { value },
            set: { n in
                // The write is a durable file replace, awaited off the main
                // thread; the config mirror re-renders once it lands.
                Task {
                    do {
                        try await setValue(n)
                    }
                    catch {
                        showError(error)
                    }
                }
            }
        )
    }
}
