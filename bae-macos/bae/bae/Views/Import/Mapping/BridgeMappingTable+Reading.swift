import BaeKit
import Foundation

/// Reading the mapping table. Every value here is either one core already
/// decided — which track a row commits, what its source is, which catalog
/// message names the tally — or a count over the table's own rows.

extension BridgeTrackMapping {
    /// A row whose track nobody has named.
    var isUnanswered: Bool {
        track.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }
}

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

extension BridgeMappingTrackSection {
    /// The source-to-track mappings this section carries.
    var mappings: [BridgeTrackMapping] {
        switch content {
        case .tracks(let mappings): mappings
        case .sheet(_, let entries): entries
        }
    }

    var sideHeaderText: String {
        side.headerText(key: headerKey)
    }
}

extension BridgeMappingTable {
    /// Every source-to-track mapping in table order.
    var trackMappings: [BridgeTrackMapping] {
        trackSections.flatMap(\.mappings)
    }

    /// Rows whose track's title is still blank.
    var unansweredCount: Int {
        trackMappings.count(where: \.isUnanswered)
    }
}

extension BridgeMappingFile {
    /// The file's size formatted for the current locale.
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
    }
}

extension BridgeSheetBound {
    var descriptionText: String {
        switch self {
        case .describes(let container): container.name
        case .describesFiles(let audioFileCount):
            coreString("ui.import.sheet.audio_files", Int(audioFileCount))
        case .unresolved(let requested):
            if requested.isEmpty {
                coreString("ui.import.sheet.describes_nothing")
            }
            else {
                coreString(
                    "ui.import.sheet.asked_for",
                    requested.formatted(.list(type: .and))
                )
            }
        case .refusedCodec(let codec):
            coreString(bridgeSheetRefusedCodecKey(), codec)
        case .refusedTiming:
            coreString("core.import.sheet.refused_timing")
        }
    }
}

extension BridgeMappingRole {
    /// The same role the scan proposed, which is what carries the localization
    /// key. A mapping row's role is that role narrowed to the ones a row can
    /// hold, so every case has an exact counterpart.
    var fileRole: BridgeFileRole {
        switch self {
        case .audio: .audio
        case .document: .document
        case .other: .other
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
                "\(releaseDurationText(sourceMs)) → \(releaseDurationText(metadataMs))"
        case (_, let metadataMs?):
            return releaseDurationText(metadataMs)
        case (let sourceMs?, nil):
            return releaseDurationText(sourceMs)
        case (nil, nil):
            return releaseDurationText(nil as UInt64?)
        }
    }
}
