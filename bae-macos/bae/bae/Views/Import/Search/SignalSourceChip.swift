import BaeKit
import SwiftUI

/// Where a value came from, as a chip beside it: `LOG`, `CUE`, `TXT`, the
/// folder or file name, or — for a value read off the artwork — the crop of
/// the image it was read from. Hovering a text chip names its file; hovering
/// a crop enlarges it with its filename.
struct SignalSourceChip: View {
    let source: BridgeValueSource
    /// Where each of the candidate's files is on disk, by the path the source
    /// names it by.
    let filePaths: [String: String]

    private var fileName: String? {
        source.file.map(lastPathComponent)
    }

    var body: some View {
        switch source.origin {
        case .artwork:
            artwork
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

    /// The crop of the image the value was read off, where the image is one
    /// of the candidate's files. A library release's stored cover is no file
    /// of a folder, so it gets the artwork origin's name instead.
    @ViewBuilder
    private var artwork: some View {
        if let file = source.file, let path = filePaths[file] {
            let content = ImageContent.localFile(path: path)
            ArtworkCropView(
                content: content,
                region: source.region,
                width: 16,
                height: 12
            )
            .hoverPopover(arrowEdge: .bottom) {
                VStack(alignment: .leading, spacing: 5) {
                    ArtworkCropView(
                        content: content,
                        region: source.region,
                        width: 132,
                        height: nil
                    )
                    Text(lastPathComponent(file))
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .frame(maxWidth: 132, alignment: .leading)
                }
                .padding(6)
                .popoverEntrance(anchor: .top)
                .background { PopoverBehavior() }
            }
        }
        else {
            Image(systemName: "photo")
                .font(.system(size: 9))
                .foregroundStyle(.secondary)
                .frame(width: 16, height: 12)
                .help(SignalBadgeStyle.originLabel(for: .artwork))
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
                ),
                filePaths: [:]
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .artwork,
                    file: nil,
                    region: nil
                ),
                filePaths: [:]
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .folderName,
                    file: nil,
                    region: nil
                ),
                filePaths: [:]
            )
            SignalSourceChip(
                source: BridgeValueSource(
                    origin: .textFile,
                    file: "info.txt",
                    region: nil
                ),
                filePaths: [:]
            )
        }
        .padding()
        .windowBackground()
    }
#endif
