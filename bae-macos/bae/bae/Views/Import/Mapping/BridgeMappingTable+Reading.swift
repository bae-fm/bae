import BaeKit
import Foundation

/// Reading the mapping table. Every value here is one core already decided —
/// which track a row commits, what its source is, which catalog message names
/// the tally — or the table's own rows walked in order.

extension BridgeMappingSource {
    /// The playing time the folder itself offers for this row: measured off
    /// the file, or stated by the sheet for one of its entries.
    var durationMs: UInt64? {
        switch self {
        case .file(let file): file.durationMs
        case .sheetEntry(let entry): entry.durationMs
        }
    }

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
    /// A container's name is cut like a folder name; every other bound reads
    /// as a sentence.
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

extension BridgeTrackMapping {
    /// The value this row exposes in the Length column and to accessibility:
    /// one duration when the facts agree, source → metadata when core says
    /// they do not.
    var displayedDuration: String {
        switch (source.durationMs, durationMs) {
        case (let sourceMs?, let metadataMs?) where lengthsDisagree:
            return
                String(
                    localized:
                        "\(releaseDurationText(sourceMs)) → \(releaseDurationText(metadataMs))"
                )
        case (_, let metadataMs?):
            return releaseDurationText(metadataMs)
        case (let sourceMs?, nil):
            return releaseDurationText(sourceMs)
        case (nil, nil):
            return releaseDurationText(nil as UInt64?)
        }
    }
}
