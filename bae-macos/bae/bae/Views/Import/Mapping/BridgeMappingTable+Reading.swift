import BaeKit
import Foundation

/// Reading the track table. Every value here is one core already decided —
/// which track a row commits, what its source is, which catalog message names
/// the tally — or the table's own rows walked in order.

extension BridgeTrackTableRun {
    /// The rows this run draws, in table order.
    var rows: [BridgeTrackTableRow] {
        switch self {
        case .track(let row): [row]
        case .sheet(_, let rows): rows
        }
    }
}

extension BridgeTrackAudition {
    /// The window of a folder's file the preview player plays for the row;
    /// `nil` for a library release's row, which the library plays.
    var previewTarget: BridgePreviewTarget? {
        switch self {
        case .preview(let target): target
        case .libraryTrack: nil
        }
    }
}

extension BridgeMappingFile {
    /// The file's size formatted for the current locale.
    var sizeText: String {
        size.fileSizeText
    }
}

extension BridgeSheetBound {
    /// The row cuts a container's name in the middle, like a file name, and
    /// every other bound at the end.
    var lineCut: LineCut {
        switch self {
        case .describes: .fileName
        case .describesFiles, .unresolved, .refusedCodec, .refusedTiming:
            .prose
        }
    }

    var descriptionText: String {
        switch self {
        case .describes(let container): container.name
        case .describesFiles(let audioFileCount):
            localizedCoreString(
                "ui.import.sheet.audio_files",
                Int(audioFileCount)
            )
        case .unresolved(let requested):
            if requested.isEmpty {
                localizedCoreString("ui.import.sheet.describes_nothing")
            }
            else {
                localizedCoreString(
                    "ui.import.sheet.asked_for",
                    requested.formatted(.list(type: .and))
                )
            }
        case .refusedCodec(let codec):
            localizedCoreString(bridgeSheetRefusedCodecKey(), codec)
        case .refusedTiming:
            localizedCoreString("core.import.sheet.refused_timing")
        }
    }
}
