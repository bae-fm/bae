import BaeKit
import Foundation

/// Reading the mapping table. Every value here is one core already decided —
/// which track a row commits, what its source is, which catalog message names
/// the tally — or the table's own rows walked in order.

extension BridgeMappingSource {
    /// The exact local source window auditioning this row plays.
    var previewTarget: BridgePreviewTarget? {
        switch self {
        case .file(let file): file.previewTarget
        case .sheetEntry(let entry): entry.previewTarget
        }
    }
}

extension BridgeMappingTrackRun {
    /// The source-to-track mappings this run carries.
    var mappings: [BridgeTrackMapping] {
        switch self {
        case .tracks(let mappings): mappings
        case .sheet(_, let entries): entries
        }
    }
}

extension BridgeMappingTrackSide {
    /// The source-to-track mappings on this side, in table order.
    var mappings: [BridgeTrackMapping] {
        runs.flatMap(\.mappings)
    }

    var sideHeaderText: String {
        side.headerText(key: headerKey)
    }
}

extension BridgeMappingTable {
    /// Every source-to-track mapping in table order.
    var trackMappings: [BridgeTrackMapping] {
        trackSides.flatMap(\.mappings)
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
