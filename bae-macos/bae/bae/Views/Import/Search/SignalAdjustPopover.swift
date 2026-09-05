import BaeKit
import SwiftUI

/// What the header's Adjust opens: one row per signal core extracted, and the
/// way to run the lookups again.
///
/// The disc ID and the barcode are checkboxes — they are in the run until
/// unchecked. The catalog opens the numbers found in the folder, because the
/// run looks up the one it is told to. A verdict resumed from the store has no
/// signals to show, so the popover is Run again alone.
///
/// Every row draws itself: an AppKit checkbox or menu renders its label as a
/// control title, which would drop the value and the count.
struct SignalAdjustPopover: View {
    let toolbar: BridgeSignalsToolbar
    let onToggle: (BridgeSignalToggle) -> Void
    let onRerun: () -> Void

    /// Which signal's options are open. The catalog's numbers expand in place
    /// rather than in a second popover over this one.
    @State
    private var expanded: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
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
        Button {
            guard let toggle = BridgeSignalToggle(signal: signal) else {
                return
            }
            onToggle(toggle)
        } label: {
            HStack(spacing: 7) {
                SignalCheckbox(isOn: !signal.excluded)
                rowLabel(signal, value: signal.value)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(helpLine(signal))
    }

    /// The catalog: its numbers open under it, and picking one closes them.
    @ViewBuilder
    private func catalogRow(_ signal: BridgeToolbarSignal) -> some View {
        Button {
            expanded = expanded == signal.id ? nil : signal.id
        } label: {
            HStack(spacing: 7) {
                SignalCheckbox(isOn: signal.value != nil && !signal.excluded)
                rowLabel(signal, value: signal.value)
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.secondary)
                    .rotationEffect(
                        .degrees(expanded == signal.id ? 180 : 0)
                    )
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(signal.options.isEmpty)
        .help(helpLine(signal))
        if expanded == signal.id {
            CatalogOptionsList(options: signal.options) { value in
                expanded = nil
                onToggle(.catalog(value: value))
            }
            .padding(.leading, 20)
        }
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
                .foregroundStyle(.primary)
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
            CountCapsule(text: signal.options.count.formatted(), matched: false)
        }
        else {
            switch signal.state {
            case .found(let count):
                CountCapsule(text: count.formatted(), matched: true)
            case .noMatch:
                CountCapsule(text: 0.formatted(), matched: false)
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
