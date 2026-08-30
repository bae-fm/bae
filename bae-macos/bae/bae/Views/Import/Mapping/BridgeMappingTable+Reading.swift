import BaeKit
import Foundation

/// Reading and editing the mapping table. Every value here is either one core
/// already decided — which track a row commits, what its source is, which
/// catalog message names the tally — or a count over the table's own rows.
/// Nothing here works out a pairing.

extension BridgeMappingUnit {
    /// The track this row commits, where it commits one.
    var track: BridgeRawTrackEdit? {
        guard case .track(let track, _) = becomes else { return nil }
        return track
    }

    /// The position the picked release names for this row's track.
    var sourcePosition: String? {
        guard case .track(_, let position) = becomes else { return nil }
        return position
    }

    /// Whether committing writes a track for this row: the rows carrying audio
    /// do, and a track the folder has nothing behind is carried without one.
    var writesTrack: Bool { track?.file != nil }

    /// A row that will write a track nobody has named.
    var isUnanswered: Bool {
        guard writesTrack, let track else { return false }
        return track.title.trimmingCharacters(in: .whitespacesAndNewlines)
            .isEmpty
    }
}

extension BridgeMappingSource {
    /// The file on disk this row's audio lives in — the file itself, or the
    /// container a sheet entry is carved out of. `nil` for a track the release
    /// names that the folder has nothing for.
    var audioPath: String? {
        switch self {
        case .file(let file): file.localPath
        case .sheetEntry(let entry): entry.containerLocalPath
        case .missing: nil
        }
    }

    /// The playing time the folder itself offers for this row: measured off
    /// the file, or stated by the sheet for one of its entries.
    var durationMs: UInt64? {
        switch self {
        case .file(let file): file.durationMs
        case .sheetEntry(let entry): entry.durationMs
        case .missing: nil
        }
    }

}

extension BridgeMappingRow {
    /// The units this row carries: itself, or the entries a sheet carves.
    var units: [BridgeMappingUnit] {
        switch self {
        case .unit(let unit): [unit]
        case .sheet(_, let entries): entries
        case .directory: []
        }
    }
}

extension BridgeMappingTable {
    /// Every unit the table carries, top-level rows and sheet entries alike, in
    /// the order the table lays them out.
    var units: [BridgeMappingUnit] { rows.flatMap(\.units) }

    /// Rows that will write a track.
    var willWriteCount: Int { units.count(where: \.writesTrack) }

    /// Rows that will write a track whose title is still blank.
    var unansweredCount: Int { units.count(where: \.isUnanswered) }

    /// Every audio unit the table's rows carry, in table order — what a row
    /// with nothing behind it is offered to point at.
    var audioChoices: [ImportAudioChoice] {
        units.compactMap(ImportAudioChoice.init(unit:))
    }

}

/// One of the folder's audio units as the "Choose file…" menu offers it: what
/// picking it writes onto a row, and what to call it.
struct ImportAudioChoice: Identifiable {
    let audio: BridgeAudioFile
    /// The container's name, and for a sheet entry the entry it names.
    let label: String

    var id: BridgeAudioFile { audio }

    init?(unit: BridgeMappingUnit) {
        guard let audio = unit.track?.file else { return nil }
        self.audio = audio
        switch unit.source {
        case .file(let file):
            label = "\(file.name), \(file.sizeText)"
        case .sheetEntry(let entry):
            label = "\(entry.containerName), \(entry.number)"
        case .missing:
            return nil
        }
    }
}

extension BridgeMappingFile {
    /// The file's size formatted for the current locale.
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
    }
}

extension BridgeMappingContainer {
    /// The container's size formatted for the current locale.
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
    }
}

extension BridgeSheetBound {
    /// The audio the sheet is on, where it is on any.
    private var container: BridgeMappingContainer? {
        switch self {
        case .describes(let container): container
        case .refusedCodec(let container, _): container
        case .unresolved: nil
        }
    }

    var containerId: String? { container?.fileId }

    var containerName: String? { container?.name }

    var containerSizeText: String? { container?.sizeText }

    /// Why the sheet is on no audio, in the user's language — what its
    /// directive asked for, or the codec bae cannot carve tracks out of. `nil`
    /// when it is on audio and there is nothing to explain.
    var reasonLine: String? {
        switch self {
        case .describes:
            nil
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
        case .refusedCodec(_, let codec):
            coreString(bridgeSheetRefusedCodecKey(), codec)
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

/// The tally above the mapping table, in the user's language, or nothing where
/// there is no line to draw — core says which by naming a key or not, and two
/// sides that account for the same rows name none. Each message takes its own
/// numbers, in the order the English value names them.
func bridgeSlotReconciliationText(
    _ value: BridgeSlotReconciliation
) -> String? {
    guard let key = bridgeSlotReconciliationKey(reconciliation: value) else {
        return nil
    }
    switch value {
    case .agrees:
        return nil
    case .moreFiles(let files, let tracks),
        .moreTracks(let files, let tracks):
        return coreString(key, Int(files), Int(tracks))
    }
}

/// A duration in milliseconds as a clock label, or an em dash where there is no
/// number. Never a zero: an unknown length and a zero-length file are different
/// facts, and only one of them is real.
func importDurationText(_ ms: UInt64?) -> String {
    guard let ms else { return "\u{2014}" }
    let label = DurationClock.text(Int64(ms))
    return label.isEmpty ? "\u{2014}" : label
}

extension BridgeMappingUnit {
    /// The value this row exposes in the Length column and to accessibility.
    var displayedDuration: String {
        importDurationText(durationMs)
    }
}

extension BridgeMappingUnit {
    /// Whether this unit is one of the release's tracks — settled as one, or
    /// audio waiting for a release to name it. Both belong in the Tracks
    /// section: the table is the same table before and after a pick, and a
    /// folder's audio does not move sections when one lands.
    var isTrack: Bool {
        switch becomes {
        case .track, .awaitingPick: return true
        case .kept: return false
        }
    }
}

extension BridgeMappingTable {
    /// The rows carried with the release that are not its tracks: the files
    /// with a role and nothing to become, and the directories that collapse to
    /// one row. What the Files section lists.
    var keptRows: [BridgeMappingRow] {
        rows.filter { row in
            switch row {
            case .unit(let unit): return !unit.isTrack
            case .directory: return true
            case .sheet: return false
            }
        }
    }
}
