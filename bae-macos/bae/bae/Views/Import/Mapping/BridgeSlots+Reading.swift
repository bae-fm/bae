import BaeKit
import Foundation

/// Reading the slot table's typed values. Every one of these is a projection of
/// what core already decided — which file a row is bound to, what the source
/// says about it, which catalog message names the tally. Nothing here works out
/// a pairing or a count of its own.

extension BridgeAudioFile {
    /// The file on disk this audio unit lives in — the whole file, or the
    /// container a track sheet carves this slice out of. The id
    /// `setFileRole` takes, and what an exclusion matches slot rows against.
    var fileId: String {
        switch self {
        case .standalone(let fileId): fileId
        case .sheetSlice(let fileId, _, _): fileId
        }
    }
}

extension BridgeTrackSlot {
    /// The audio core bound to this row; `nil` for a track the source names
    /// with nothing on disk behind it.
    var boundFile: BridgeSlotFile? {
        switch self {
        case .paired(_, _, _, let file): file
        case .fileOnly(_, let file): file
        case .trackOnly: nil
        }
    }

    /// The source's own position string — `A1`, `1`, `1-2`, or prose. `nil` for
    /// audio the source does not name, which has no position because the source
    /// says nothing about it.
    var position: String? {
        switch self {
        case .paired(_, let position, _, _): position
        case .trackOnly(_, let position, _): position
        case .fileOnly: nil
        }
    }

    /// The length the source states for this track, where it states one.
    var sourceDurationMs: UInt64? {
        switch self {
        case .paired(_, _, let ms, _): ms
        case .trackOnly(_, _, let ms): ms
        case .fileOnly: nil
        }
    }

}

/// The tally above the slot table, in the user's language. Each message takes
/// its own numbers, in the order the English value names them — one when the
/// two sides agree, since both have the same one.
func bridgeSlotReconciliationText(_ value: BridgeSlotReconciliation) -> String {
    let key = bridgeSlotReconciliationKey(reconciliation: value)
    switch value {
    case .agrees(let count):
        return coreString(key, Int(count))
    case .moreFiles(let files, let tracks),
        .moreTracks(let files, let tracks):
        return coreString(key, Int(files), Int(tracks))
    }
}

/// What a file's role makes of it, in the user's language — the roles table's
/// "Becomes" column. A single slot and a run of slots are different sentences,
/// so core hands back a different key for each.
func bridgeFileBecomesText(_ becomes: BridgeFileBecomes) -> String {
    let key = bridgeFileBecomesKey(becomes: becomes)
    switch becomes {
    case .slots(let first, let last) where first == last:
        return coreString(key, Int(first))
    case .slots(let first, let last):
        return coreString(key, Int(first), Int(last))
    case .noSlots:
        return coreString(key)
    }
}

/// What a collapsed directory holds, in the user's language — "14 images".
func bridgeFileRowKindText(_ kind: BridgeFileRowKind, count: UInt32) -> String {
    coreString(bridgeFileRowKindKey(kind: kind), Int(count))
}

extension BridgeSlotFile {
    /// The container's size formatted for the current locale. A sheet slice has
    /// no size of its own on disk, so this is the whole container's — core says
    /// so, and the row shows what core says.
    var sizeText: String {
        Int64(size).formatted(.byteCount(style: .file))
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
