import BaeKit
import SwiftUI

/// The run as the steps it is taking — the disc ID, the artwork, the barcode,
/// the catalog number — one row each, with every provider's part of a step
/// nested under it. Each row settles on its own, so a person watches the run
/// rather than waiting for it, and a provider that failed offers its own
/// Retry while the others carry on.
///
/// The catalog row is where the run is told which number to look up: its
/// picker opens the numbers extraction found, and choosing one starts that
/// lookup. Excluding a signal stays in Adjust.
struct IdentifyRunStepsView: View {
    let run: BridgeIdentifyRun
    /// The catalog numbers extraction found, for the row's picker.
    let catalogOptions: [BridgeSignalOption]
    let onToggleSignal: (BridgeSignalToggle) -> Void
    /// Re-ask only the lookups that failed.
    let onRetryFailed: () -> Void

    @State
    var isPickingCatalog = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            FindOnlineCapsLabel("Automatic")
                .padding(.top, 10)
                .padding(.leading, 14)
                .padding(.bottom, 4)
            discIdRows
            artworkRow
            barcodeRows
            catalogRows
        }
        .padding(.bottom, 10)
    }
}

/// How a step is going, drawn at the row's left edge.
enum StepGlyph {
    /// Not started: waits on an earlier step.
    case waiting
    case working
    case done
    /// Never runs for this folder.
    case none
    case failed
}

/// One row of the run: the glyph, the step's name, the value it carries,
/// where it is in a sequence, and what it has to say about itself at the
/// right edge. Provider rows nest one level under their step.
struct StepRow<Trailing: View>: View {
    let glyph: StepGlyph
    let label: String
    var value: String?
    var position: String?
    var nested = false
    var dimmed = false
    @ViewBuilder
    let trailing: () -> Trailing

    var body: some View {
        HStack(spacing: 8) {
            glyphView
                .frame(width: 14, height: 14)
            Text(label)
                .font(.system(size: 12.5))
                .foregroundStyle(
                    dimmed ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.primary)
                )
                .lineLimit(1)
                .fixedSize()
            if let value {
                Text(value)
                    .font(.system(size: 10.5, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .frame(maxWidth: 200, alignment: .leading)
            }
            if let position {
                Text(position)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.tertiary)
                    .lineLimit(1)
                    .fixedSize()
            }
            Spacer(minLength: 8)
            trailing()
        }
        .frame(height: 24)
        .padding(.leading, nested ? 34 : 14)
        .padding(.trailing, 14)
    }

    @ViewBuilder
    private var glyphView: some View {
        switch glyph {
        case .waiting:
            Circle()
                .fill(.white.opacity(0.18))
                .frame(width: 5, height: 5)
        case .working:
            ProgressView()
                .controlSize(.small)
                .scaleEffect(0.6)
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 11))
                .foregroundStyle(.green)
        case .none:
            RoundedRectangle(cornerRadius: 1)
                .fill(.white.opacity(0.25))
                .frame(width: 8, height: 1.5)
        case .failed:
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 10))
                .foregroundStyle(.orange)
        }
    }
}

#if DEBUG
    // MARK: - Previews

    #Preview("Run in flight") {
        IdentifyRunStepsView(
            run: PreviewData.identifyRunInFlight,
            catalogOptions: PreviewData.toolbarCatalogChoices.signals[1]
                .options,
            onToggleSignal: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .windowBackground()
    }

    #Preview("Run starting") {
        IdentifyRunStepsView(
            run: PreviewData.identifyRunStarting,
            catalogOptions: [],
            onToggleSignal: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .windowBackground()
    }

    #Preview("A provider failed") {
        IdentifyRunStepsView(
            run: PreviewData.identifyRunProviderFailed,
            catalogOptions: PreviewData.toolbarCatalogChoices.signals[1]
                .options,
            onToggleSignal: { _ in },
            onRetryFailed: {},
        )
        .frame(width: 660)
        .windowBackground()
    }
#endif
