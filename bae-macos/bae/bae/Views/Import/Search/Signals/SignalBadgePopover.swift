import BaeKit
import SwiftUI

/// The hover popover for a badge: where the signal came from, the full
/// untruncated value, its current state, and the click-to-toggle hint.
struct SignalBadgePopover: View {
    let signal: BridgeToolbarSignal

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 6) {
                Image(systemName: SignalBadgeStyle.icon(for: signal.kind))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Text(
                    "\(SignalBadgeStyle.label(for: signal.kind)) · from \(SignalBadgeStyle.originLabel(for: signal.origin))"
                )
                .font(.system(size: 11, weight: .semibold))
                .tracking(0.5)
                .textCase(.uppercase)
                .foregroundStyle(.tertiary)
            }

            if let value = signal.value {
                Text(value)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }

            Divider()

            HStack(spacing: 6) {
                Circle()
                    .fill(SignalBadgeStyle.stateDotColor(for: signal))
                    .frame(width: 5, height: 5)
                Text(SignalBadgeStyle.stateLabel(for: signal))
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 12)
                Text(signal.excluded ? "Click to include" : "Click to exclude")
                    .font(.system(size: 11.5))
                    .foregroundStyle(Theme.accent)
            }
        }
        .padding(11)
        .frame(width: 246)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Badge popover") {
        SignalBadgePopover(
            signal: BridgeToolbarSignal(
                kind: .catalog,
                value: "WPCR-80001",
                origin: .folderName,
                state: .found(count: 1),
                excluded: false,
                options: []
            )
        )
        .padding()
        .windowBackground()
    }
#endif
