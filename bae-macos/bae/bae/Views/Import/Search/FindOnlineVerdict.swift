import BaeKit
import Foundation

/// The one-line answer the Find online header gives about identification, and
/// the action offered beside it.
///
/// Pure presentation over what core already shaped: the identify state names
/// the situation and `BridgeSignalsToolbar` names the signals in it. Nothing
/// here decides anything about a lookup — it only words what core settled.
struct FindOnlineVerdict: Equatable {
    /// What the action beside the line does. `none` for a state with nothing
    /// to act on: a run still going, or a folder with no signals to adjust.
    enum Action: Equatable {
        case none
        /// Start identification for a folder whose run never began.
        case identify
        /// Open the signal popover.
        case adjust
        /// Re-run the lookups that failed.
        case retry
    }

    /// The verdict itself. A failure names one source per line, so several
    /// providers failing reads as several lines rather than one run-on.
    let lines: [String]
    /// Whether a run is under way, so the header spins beside its line.
    let isWorking: Bool
    /// Whether the lines report a failure, so the header warns rather than
    /// states.
    let isFailure: Bool
    let action: Action
    /// The full reason behind a failure line, for the row's help. Empty when
    /// the line is the whole of what there is to say.
    let help: String

    private init(
        lines: [String],
        isWorking: Bool = false,
        isFailure: Bool = false,
        action: Action,
        help: String = ""
    ) {
        self.lines = lines
        self.isWorking = isWorking
        self.isFailure = isFailure
        self.action = action
        self.help = help
    }

    init(state: IdentifyState, toolbar: BridgeSignalsToolbar) {
        switch state {
        case .idle:
            self.init(
                lines: [String(localized: "Not identified")],
                action: .identify
            )
        case .triangulating:
            self.init(
                lines: [String(localized: "Identifying\u{2026}")],
                isWorking: true,
                action: .none
            )
        case .found:
            self.init(lines: [Self.identifiedLine(toolbar)], action: .adjust)
        case .notFoundAnywhere:
            self.init(lines: [Self.noMatchesLine(toolbar)], action: .adjust)
        case .manualOnly:
            self.init(
                lines: [String(localized: "No signals in this folder")],
                action: .none
            )
        case .failed(let failures, _, _, _):
            self.init(
                lines: failures.map(\.verdictLine),
                isFailure: true,
                action: .retry,
                help: failures.map(\.badgeLine).joined(separator: "\n")
            )
        }
    }

    // MARK: - Wording

    /// "Identified by Disc ID and barcode" — the signals that matched. A
    /// verdict stood back up from the store has no signals to name.
    private static func identifiedLine(
        _ toolbar: BridgeSignalsToolbar
    ) -> String {
        let matched = sentenceLabels(of: toolbar) {
            if case .found = $0.state {
                true
            }
            else {
                false
            }
        }
        return matched.isEmpty
            ? String(localized: "Identified")
            : String(localized: "Identified by \(andList(matched))")
    }

    /// "No matches for Disc ID or barcode" — the signals that ran. A signal
    /// that was skipped never asked, so it is not part of the answer.
    private static func noMatchesLine(
        _ toolbar: BridgeSignalsToolbar
    ) -> String {
        let ran = sentenceLabels(of: toolbar) { $0.state != .skipped }
        return ran.isEmpty
            ? String(localized: "No matches")
            : String(localized: "No matches for \(orList(ran))")
    }

    /// The signals the verdict names, in toolbar order: those still in the run
    /// that pass `include`. An excluded signal took itself out, so it is not
    /// part of what the run answered.
    private static func sentenceLabels(
        of toolbar: BridgeSignalsToolbar,
        where include: (BridgeToolbarSignal) -> Bool
    ) -> [String] {
        toolbar.signals
            .filter { !$0.excluded && include($0) }
            .map { SignalBadgeStyle.sentenceLabel(for: $0.kind) }
    }

    /// "Disc ID and barcode" — the locale's own way of joining a list.
    private static func andList(_ items: [String]) -> String {
        ListFormatter.localizedString(byJoining: items)
    }

    /// "Disc ID or barcode". No formatter offers a disjunction, so the pattern
    /// joins two at a time and the catalog owns the word between them.
    private static func orList(_ items: [String]) -> String {
        guard var joined = items.first else { return "" }
        for item in items.dropFirst() {
            joined = String(localized: "\(joined) or \(item)")
        }
        return joined
    }
}

/// One lookup a provider was asked for: which step of identification, at
/// which source. A failure names one; the same source may well have answered
/// the other steps, and those matches are on the list.
struct FailedSearch: Hashable {
    let source: BridgeMetadataSource
    let step: BridgeSignalKind
}

extension BridgeIdentifyFailure {
    /// The header's short line: which step failed, at which source, and
    /// briefly why — "MusicBrainz barcode search: busy (503)". The full
    /// reason rides in the header's help (`badgeLine`).
    var verdictLine: String {
        switch self {
        case .discId(let failure):
            return searchFailed(
                FailedSearch(source: .musicBrainz, step: .discId),
                failure
            )
        case .barcode(let source, let failure):
            return searchFailed(
                FailedSearch(source: source, step: .barcode),
                failure
            )
        case .catalog(let source, let failure):
            return searchFailed(
                FailedSearch(source: source, step: .catalog),
                failure
            )
        case .barcodeScan:
            return String(localized: "Couldn't read the folder's barcodes")
        case .releaseDetails(let failure):
            return String(
                localized:
                    "Couldn't load the release details: \(failure.briefLine)"
            )
        }
    }

    /// The lookup this failure names, for the line saying its results are
    /// missing from the list. `nil` for the steps no provider owns. The
    /// disc-ID endpoint is MusicBrainz's alone, so a disc-ID failure names it.
    var failedSearch: FailedSearch? {
        switch self {
        case .discId: FailedSearch(source: .musicBrainz, step: .discId)
        case .barcode(let source, _):
            FailedSearch(source: source, step: .barcode)
        case .catalog(let source, _):
            FailedSearch(source: source, step: .catalog)
        case .barcodeScan, .releaseDetails: nil
        }
    }

    private func searchFailed(
        _ search: FailedSearch,
        _ failure: BridgeLookupFailure
    ) -> String {
        let source = bridgeMetadataSourceName(source: search.source)
        let step = SignalBadgeStyle.sentenceLabel(for: search.step)
        return String(
            localized: "\(source) \(step) search: \(failure.briefLine)"
        )
    }
}
