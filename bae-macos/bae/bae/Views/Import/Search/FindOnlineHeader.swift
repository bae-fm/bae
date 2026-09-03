import BaeKit
import SwiftUI

/// The Find online title row: the way back, the page's name, and — on the
/// right — what identification has to say plus the one thing to do about it.
///
/// The verdict never moves: a typed search replaces the result area under this
/// row, so what identification concluded stays readable while a search runs.
struct FindOnlineHeader: View {
    let verdict: FindOnlineVerdict
    let toolbar: BridgeSignalsToolbar
    /// Leave the pane. `nil` for a surface that owns its own way out — the
    /// re-identify sheet closes rather than going back.
    let onBack: (() -> Void)?
    let onIdentify: () -> Void
    let onRetry: () -> Void
    let onToggleSignal: (BridgeSignalToggle) -> Void
    let onRerun: () -> Void

    @State
    private var isAdjusting = false

    var body: some View {
        HStack(spacing: 12) {
            if let onBack {
                Button(action: onBack) {
                    Label("Back", systemImage: "chevron.left")
                }
                .buttonStyle(.link)
                .font(.system(size: 13))
                Rectangle()
                    .fill(.white.opacity(0.1))
                    .frame(width: 1, height: 14)
            }
            Text("Find online")
                .font(.system(size: 13, weight: .semibold))
            Spacer(minLength: 12)
            verdictLine
            action
        }
        .padding(.horizontal, 14)
        .frame(height: 42)
    }

    private var verdictLine: some View {
        HStack(spacing: 6) {
            if verdict.isWorking {
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.7)
            }
            if verdict.isFailure {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 11))
            }
            VStack(alignment: .trailing, spacing: 1) {
                ForEach(verdict.lines, id: \.self) { line in
                    Text(line)
                }
            }
        }
        .font(.system(size: 12))
        .foregroundStyle(verdict.isFailure ? Color.orange : .secondary)
        .lineLimit(1)
        .help(verdict.help)
    }

    @ViewBuilder
    private var action: some View {
        switch verdict.action {
        case .none:
            EmptyView()
        case .identify:
            Button("Identify", action: onIdentify)
                .buttonStyle(.link)
                .font(.system(size: 12))
        case .retry:
            Button("Retry", action: onRetry)
                .buttonStyle(.link)
                .font(.system(size: 12))
        case .adjust:
            Button("Adjust") { isAdjusting = true }
                .buttonStyle(.link)
                .font(.system(size: 12))
                .popover(isPresented: $isAdjusting, arrowEdge: .bottom) {
                    SignalAdjustPopover(
                        toolbar: toolbar,
                        onToggle: { signal in
                            isAdjusting = false
                            onToggleSignal(signal)
                        },
                        onRerun: {
                            isAdjusting = false
                            onRerun()
                        },
                    )
                    .popoverEntrance(anchor: .top)
                    .background { PopoverBehavior() }
                }
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Header verdicts") {
        VStack(spacing: 0) {
            let states = [
                PreviewData.searchStateFoundExact,
                PreviewData.searchStateTriangulating,
                PreviewData.searchStateNotFound,
                PreviewData.searchStateNoSignals,
                PreviewData.searchStateSourceFailure,
                PreviewData.searchStateIdle,
            ]
            ForEach(Array(states.enumerated()), id: \.offset) { _, state in
                FindOnlineHeader(
                    verdict: FindOnlineVerdict(
                        state: state.identifyState,
                        toolbar: state.signalsToolbar
                    ),
                    toolbar: state.signalsToolbar,
                    onBack: {},
                    onIdentify: {},
                    onRetry: {},
                    onToggleSignal: { _ in },
                    onRerun: {},
                )
                Divider()
            }
        }
        .frame(width: 660)
        .windowBackground()
    }
#endif
