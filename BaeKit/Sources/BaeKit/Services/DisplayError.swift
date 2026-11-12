import Foundation

/// A typed core failure that renders to a localized line plus optional opaque
/// detail. Both `BridgeError` and `BridgePlaybackErrorReason` satisfy it (they
/// expose `localizedLine`/`shownDetail` in `BridgeError+Localized`), so a
/// `DisplayError` builds from either uniformly.
///
/// Both are optional because core decides them: whether a failure has a line
/// at all (a cancellation has none, and `nil` means show nothing), and whether
/// its detail says anything that line doesn't (`nil` shows the line alone).
public protocol LocalizedFailure {
    var localizedLine: String? { get }
    var shownDetail: String? { get }
    var retryRuledOut: Bool { get }
}

extension BridgeError: LocalizedFailure {}
extension BridgePlaybackErrorReason: LocalizedFailure {}

/// A user-facing error ready to render: a localized line plus, for diagnostics,
/// the opaque Rust error chain a notice offers behind Details, to copy (never
/// translated). UI-originated errors (a failed drop, a caught Swift error) are
/// already localized prose and carry no detail; core errors crossing the bridge
/// arrive as typed reasons and render their generic per-category line here.
public struct DisplayError: Equatable, Sendable {
    private static let detailExcerptLength = 400
    private static let detailSummaryLength = 180

    public let line: String
    public let detail: String?
    /// Whether a surface offers Retry on this failure: always, unless core
    /// says making the same call again can't get past it — a damaged
    /// database, a bug, an edit to fix. Prose the UI wrote, and a failure
    /// core keys without saying, have nothing ruling a retry out.
    public let offersRetry: Bool

    /// The opening portion the Details alert shows. Copy continues to use
    /// `detail`, which retains the complete diagnostic.
    public var detailExcerpt: String? {
        guard let detail else { return nil }
        let excerpt = detail.prefix(Self.detailExcerptLength)
        return excerpt.endIndex == detail.endIndex ? detail : "\(excerpt)…"
    }

    /// The concrete fault as one compact line, for rendering beside `line`.
    ///
    /// `line` names a *category* — "Something went wrong." — so a surface
    /// showing only it tells the user nothing about what failed, and the fault
    /// is reachable only by whoever thinks to open Details or read the log.
    /// This is the first line of the diagnostic, bounded so it fits under a
    /// headline; `detailExcerpt` and `detail` still carry the rest for the
    /// Details alert and the copy button.
    ///
    /// `nil` when there is nothing to say: a keyed failure carries no
    /// diagnostic, a failure whose line core says names the whole fault
    /// carries none to show, and a blank one is not a fault name.
    public var detailSummary: String? {
        guard let detail else { return nil }
        let firstLine = detail.prefix { !$0.isNewline }
            .trimmingCharacters(in: .whitespaces)
        guard !firstLine.isEmpty else { return nil }
        let summary = firstLine.prefix(Self.detailSummaryLength)
        return summary.endIndex == firstLine.endIndex
            ? firstLine : "\(summary)…"
    }

    /// A UI-originated error: prose the UI already localized, with no opaque
    /// detail to offer.
    public init(line: String) {
        self.line = line
        self.detail = nil
        self.offersRetry = true
    }

    /// A failure the UI writes the headline for but core wrote the fault text
    /// of: an event carrying an untranslated diagnostic beside the thing it
    /// happened to. The headline is localized at the call site; `detail` stays
    /// verbatim for Details and the copy button, like any other core
    /// diagnostic.
    public init(line: String, detail: String?) {
        self.line = line
        self.detail = detail
        self.offersRetry = true
    }

    private init(line: String, detail: String?, offersRetry: Bool) {
        self.line = line
        self.detail = detail
        self.offersRetry = offersRetry
    }

    /// Put the operation or object that failed before the existing localized
    /// line without discarding the opaque diagnostic offered by "Copy details".
    public func addingContext(_ context: String) -> DisplayError {
        DisplayError(
            line: String(localized: "\(context): \(line)"),
            detail: detail,
            offersRetry: offersRetry
        )
    }

    /// The same failure under a line the caller words around this one —
    /// "Save failed: <line>" — keeping the diagnostic for "Copy details".
    public func rewording(_ reword: (String) -> String) -> DisplayError {
        DisplayError(
            line: reword(line),
            detail: detail,
            offersRetry: offersRetry
        )
    }

    /// A playback failure — the one typed core failure that is not a Swift
    /// `Error`, so it cannot reach `init(_ error:)`. Everything else that crosses
    /// the bridge is an `Error` and goes through there, finding this same mapping.
    ///
    /// Concrete rather than `LocalizedFailure`: an existential parameter would tie
    /// with `any Error` for a `BridgeError`, which is both.
    ///
    /// Fails when core says the failure has no line to show.
    public init?(_ failure: BridgePlaybackErrorReason) {
        guard let line = failure.localizedLine else { return nil }
        self.line = line
        self.detail = failure.shownDetail
        self.offersRetry = !failure.retryRuledOut
    }

    /// Any error on its way to a human.
    ///
    /// A failure that crossed the bridge renders core's keyed line and keeps the
    /// opaque detail core says to show beside it, for the copy button. Anything
    /// else is Swift-origin prose and passes through unchanged — which is why a
    /// `catch` can be routed here without first proving what it can throw.
    ///
    /// Route here rather than to `localizedDescription` on the way to a human. A
    /// bridge error's `localizedDescription` is core's line too, but it leaves
    /// out the opaque detail the copy button offers, and for a cancellation
    /// Foundation puts its generic sentence where core says to show nothing.
    ///
    /// Fails when core says the failure has no line to show — a cancellation. A
    /// Swift-origin error always has one: it is prose the UI wrote.
    public init?(_ error: any Error) {
        guard let failure = error as? LocalizedFailure else {
            self.line = error.localizedDescription
            self.detail = nil
            self.offersRetry = true
            return
        }
        guard let line = failure.localizedLine else { return nil }
        self.line = line
        self.detail = failure.shownDetail
        self.offersRetry = !failure.retryRuledOut
    }
}
