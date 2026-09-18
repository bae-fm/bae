import BaeKit
import SwiftUI

/// One name the object itself carries: the seal, what kind of name it is, the
/// value as it was read, and a tag per surface it was read from.
///
/// Core folds the readings — two scans showing one barcode arrive as one mark
/// tagged `scan` — so this draws what it is given.
struct MarkLine: View {
    let mark: BridgeReleaseMark

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "seal")
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
            Text(coreString(bridgeMarkKindKey(kind: mark.kind)))
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            // A disc ID is longer than the line it sits on; its head and tail
            // are what identifies it, so the middle is what goes.
            Text(mark.value)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(.primary)
                .lineLimit(1)
                .truncationMode(.middle)
            ForEach(mark.origins, id: \.self) { origin in
                OriginTag(origin: origin)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// Where one reading of a value happened, as the short tag the line ends with.
private struct OriginTag: View {
    let origin: BridgeSignalOrigin

    var body: some View {
        Text(coreString(bridgeSignalOriginKey(origin: origin)))
            .font(.system(size: 10.5))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(
                Color.secondary.opacity(0.15),
                in: RoundedRectangle(cornerRadius: 4)
            )
            .fixedSize()
    }
}

/// Every name an object carries, one line each in the order core lists mark
/// kinds. Draws nothing for an object nothing was read off.
struct MarkLines: View {
    let marks: [BridgeReleaseMark]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
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
