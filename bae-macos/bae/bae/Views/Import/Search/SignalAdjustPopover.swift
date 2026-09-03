import BaeKit
import SwiftUI

/// What the header's Adjust opens: one row per signal core extracted, and the
/// way to run the lookups again.
///
/// The disc ID and the barcode are checkboxes — they are in the run until
/// unchecked. The catalog is a menu over the numbers found in the folder,
/// because a folder can carry thirty of them and the run looks up the one it
/// is told to. A verdict resumed from the store has no signals to show, so the
/// popover is Run again alone.
struct SignalAdjustPopover: View {
    let toolbar: BridgeSignalsToolbar
    let onToggle: (BridgeSignalToggle) -> Void
    let onRerun: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 7) {
            ForEach(toolbar.signals) { signal in
                if signal.kind == .catalog {
                    catalogRow(signal)
                }
                else {
                    checkboxRow(signal)
                }
            }
            if !toolbar.signals.isEmpty {
                Divider()
            }
            Button(action: onRerun) {
                Label("Run again", systemImage: "arrow.clockwise")
                    .font(.system(size: 12))
            }
            .buttonStyle(.link)
        }
        .padding(11)
        .frame(width: 320)
    }

    // MARK: - Rows

    /// A one-value signal: checked while it is in the run.
    private func checkboxRow(_ signal: BridgeToolbarSignal) -> some View {
        Toggle(
            isOn: Binding(
                get: { !signal.excluded },
                set: { _ in
                    guard let toggle = BridgeSignalToggle(signal: signal)
                    else { return }
                    onToggle(toggle)
                }
            )
        ) {
            rowLabel(signal, value: signal.value)
        }
        .toggleStyle(.checkbox)
        .help(helpLine(signal))
    }

    /// The catalog: a menu over every number extracted from the folder. At
    /// most one is chosen — choosing another replaces it.
    private func catalogRow(_ signal: BridgeToolbarSignal) -> some View {
        Menu {
            ForEach(signal.options, id: \.value) { option in
                Toggle(
                    option.value,
                    isOn: Binding(
                        get: { option.chosen },
                        set: { _ in onToggle(.catalog(value: option.value)) }
                    )
                )
            }
        } label: {
            rowLabel(signal, value: signal.value)
        }
        .menuStyle(.borderlessButton)
        .disabled(signal.options.isEmpty)
        .help(helpLine(signal))
    }

    /// The name, the value it carries, and how many releases it named — the
    /// same three facts on every row, so the column reads straight down.
    private func rowLabel(
        _ signal: BridgeToolbarSignal,
        value: String?
    ) -> some View {
        HStack(spacing: 7) {
            Text(SignalBadgeStyle.label(for: signal.kind))
                .font(.system(size: 12, weight: .semibold))
                .fixedSize()
            if let value {
                Text(value)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 6)
            count(signal)
        }
    }

    /// The trailing capsule: how many releases the signal named, or — for a
    /// catalog with nothing chosen — how many numbers there are to choose
    /// from. It stays in the tree while a lookup runs so the rows hold still.
    private func count(_ signal: BridgeToolbarSignal) -> some View {
        ZStack(alignment: .trailing) {
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
                .opacity(signal.state == .lookingUp ? 1 : 0)
            capsule(signal)
                .opacity(signal.state == .lookingUp ? 0 : 1)
        }
        .frame(width: 26, alignment: .trailing)
    }

    @ViewBuilder
    private func capsule(_ signal: BridgeToolbarSignal) -> some View {
        if SignalBadgeStyle.awaitingChoice(signal) {
            countCapsule(signal.options.count.formatted(), matched: false)
        }
        else {
            switch signal.state {
            case .found(let count):
                countCapsule(count.formatted(), matched: true)
            case .noMatch:
                countCapsule(0.formatted(), matched: false)
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 11))
                    .foregroundStyle(.orange)
            case .skipped, .lookingUp:
                Image(systemName: "minus")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
        }
    }

    private func countCapsule(_ text: String, matched: Bool) -> some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(matched ? Color.green : Color.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(
                matched ? Color.green.opacity(0.14) : .white.opacity(0.05),
                in: RoundedRectangle(cornerRadius: 4)
            )
    }

    /// Where the value came from and what the lookup made of it — the detail
    /// the row has no width for.
    private func helpLine(_ signal: BridgeToolbarSignal) -> String {
        let origin = String(
            localized:
                "\(SignalBadgeStyle.label(for: signal.kind)) · from \(SignalBadgeStyle.originLabel(for: signal.origin))"
        )
        return origin + "\n" + SignalBadgeStyle.stateLabel(for: signal)
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Adjust popover") {
        SignalAdjustPopover(
            toolbar: PreviewData.toolbarBothMatched,
            onToggle: { _ in },
            onRerun: {},
        )
        .padding()
        .windowBackground()
    }

    #Preview("Adjust popover — catalog waiting for a pick") {
        SignalAdjustPopover(
            toolbar: PreviewData.toolbarCatalogChoices,
            onToggle: { _ in },
            onRerun: {},
        )
        .padding()
        .windowBackground()
    }

    #Preview("Adjust popover — resumed verdict") {
        SignalAdjustPopover(
            toolbar: BridgeSignalsToolbar(signals: []),
            onToggle: { _ in },
            onRerun: {},
        )
        .padding()
        .windowBackground()
    }
#endif
