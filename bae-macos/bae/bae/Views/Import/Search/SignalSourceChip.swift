import BaeKit
import SwiftUI

/// Where a value came from, as a chip beside it: `LOG`, `CUE`, `TXT`, the
/// folder or the file name, naming its file on hover. A value read off the
/// cover artwork gets no chip at all.
struct SignalSourceChip: View {
    let source: BridgeValueSource

    private var fileName: String? {
        source.file.map(lastPathComponent)
    }

    var body: some View {
        switch source.origin {
        // The cover scan the value was read off is not shown, so there is
        // nothing for a chip to name.
        case .artwork, .artworkBarcode:
            EmptyView()
        case .cueSheet:
            SignalTextChip(text: "CUE")
                .help(fileName ?? SignalBadgeStyle.originLabel(for: .cueSheet))
        case .textFile:
            SignalTextChip(text: "TXT")
                .help(fileName ?? SignalBadgeStyle.originLabel(for: .textFile))
        case .folderName:
            SignalTextChip(text: String(localized: "Folder"))
                .help(SignalBadgeStyle.originLabel(for: .folderName))
        case .filename:
            SignalTextChip(text: String(localized: "File"))
                .help(fileName ?? SignalBadgeStyle.originLabel(for: .filename))
        case .discToc:
            SignalTextChip(text: "LOG")
                .help(fileName ?? SignalBadgeStyle.originLabel(for: .discToc))
        }
    }
}

/// The file a disc ID was read off: `LOG` or `CUE`, naming the file on
/// hover.
struct DiscIdFileChip: View {
    let source: BridgeDiscIdFile

    var body: some View {
        SignalTextChip(text: source.kind == .log ? "LOG" : "CUE")
            .help(lastPathComponent(source.file))
    }
}

/// A source named by a short tag: a file format, or where the value was
/// read from. Formats keep their names; the rest are words, so they are set
/// in caps by the chip rather than written that way.
struct SignalTextChip: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 8.5, weight: .semibold, design: .monospaced))
            .tracking(0.5)
            .textCase(.uppercase)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 4)
            .padding(.vertical, 1)
            .overlay(
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(Color.primary.opacity(0.14), lineWidth: 1)
            )
            .fixedSize()
    }
}

/// The last path component of a candidate-relative path: the file a person
/// recognises, not the folder it sits in.
func lastPathComponent(_ path: String) -> String {
    path.split(separator: "/").last.map(String.init) ?? path
}

#if DEBUG
    // MARK: - Previews

    #Preview("Source chips") {
        HStack(spacing: 7) {
            DiscIdFileChip(
                source: BridgeDiscIdFile(kind: .log, file: "rip/Album.log")
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .cueSheet,
                    file: "Album.cue",
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .folderName,
                    file: nil,
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .textFile,
                    file: "info.txt",
                    region: nil
                )
            )
        }
        .padding()
        .windowBackground()
    }
#endif
