import BaeKit
import SwiftUI

/// Where a value came from, as a chip beside it: `CUE`, `TXT`, the folder
/// or the file name, `OCR` for text read off an image, or the bars a barcode
/// was decoded from — naming its file on hover. Every origin has a chip, so a
/// person can always tell where a value was read.
struct SignalSourceChip: View {
    let source: BridgeValueSource

    var body: some View {
        SignalTextChip(text: tag)
            .help(
                source.file.map(lastPathComponent)
                    ?? SignalBadgeStyle.originLabel(for: source.origin)
            )
    }

    /// The chip's text. Formats keep their names; the rest are words.
    private var tag: String {
        switch source.origin {
        case .text(origin: .cueSheet): "CUE"
        case .text(origin: .textFile): "TXT"
        case .text(origin: .artwork): "OCR"
        case .text(origin: .folderName): String(localized: "Folder")
        case .text(origin: .filename): String(localized: "File")
        case .artworkBarcode: String(localized: "Bars")
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
                    origin: .text(origin: .cueSheet),
                    file: "Album.cue",
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .text(origin: .folderName),
                    file: nil,
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .text(origin: .textFile),
                    file: "info.txt",
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .text(origin: .artwork),
                    file: "Artwork/back.jpg",
                    region: nil
                )
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .artworkBarcode,
                    file: "Artwork/back.jpg",
                    region: nil
                )
            )
        }
        .padding()
        .windowBackground()
    }
#endif
