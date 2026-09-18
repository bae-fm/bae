import BaeKit
import SwiftUI

/// One name read off the object, sealed only when its lookup found the chosen
/// release record, with a tag per surface it was read from.
///
/// Core folds the readings — two scans showing one barcode arrive as one mark
/// tagged `scan` — so this draws what it is given.
struct MarkLine: View {
    let mark: BridgeReleaseMark

    var body: some View {
        HStack(alignment: .center, spacing: 7) {
            Image(systemName: "seal")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .accessibilityLabel(coreString("core.identity.identified"))
                .opacity(mark.corroborated ? 1 : 0)
                .allowsHitTesting(mark.corroborated)
                .accessibilityHidden(!mark.corroborated)
            Text(coreString(bridgeMarkKindKey(kind: mark.kind)))
                .font(.system(size: 11.5))
                .foregroundStyle(.secondary)
            // A disc ID is longer than the line it sits on; its head and tail
            // are what identifies it, so the middle is what goes.
            Text(mark.value)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            ForEach(mark.origins, id: \.self) { origin in
                EvidenceChip(
                    label: coreString(bridgeSignalOriginKey(origin: origin)),
                    selection: .mark(
                        kind: mark.kind,
                        value: mark.value,
                        origin: origin
                    )
                )
            }
        }
        .accessibilityElement(children: .contain)
    }
}

/// Every name an object carries, one line each in the order core lists mark
/// kinds. Draws nothing for an object nothing was read off.
struct MarkLines: View {
    let marks: [BridgeReleaseMark]
    var scale: ReleaseFactsScale = .pane

    var body: some View {
        VStack(alignment: .leading, spacing: scale.lineSpacing) {
            ForEach(marks, id: \.self) { mark in
                MarkLine(mark: mark)
            }
        }
        .accessibilityIdentifier("release-marks")
    }
}

#if DEBUG

    // MARK: - Previews

    #Preview("Every kind") {
        MarkLines(marks: PreviewData.releaseMarks)
            .padding()
            .frame(width: 420)
            .background(Theme.surfaceElevated)
    }

    #Preview("One barcode, two surfaces") {
        MarkLines(marks: [PreviewData.releaseMarks[1]])
            .padding()
            .frame(width: 420)
            .background(Theme.surfaceElevated)
    }
#endif
