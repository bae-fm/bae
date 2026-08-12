import Foundation

/// A typed core failure that renders to a localized line plus optional opaque
/// detail. Both `BridgeError` and `BridgePlaybackErrorReason` satisfy it (they
/// expose `localizedLine`/`detail` in `BridgeError+Localized`), so a
/// `DisplayError` builds from either uniformly.
///
/// `localizedLine` is optional because core decides whether a failure has a line
/// at all: a cancellation has none. `nil` means show nothing.
public protocol LocalizedFailure {
    var localizedLine: String? { get }
    var detail: String? { get }
}

extension BridgeError: LocalizedFailure {}
extension BridgePlaybackErrorReason: LocalizedFailure {}

/// A user-facing error ready to render: a localized line plus, for diagnostics,
/// the opaque Rust error chain to offer in a copyable disclosure (never
/// translated). UI-originated errors (a failed drop, a caught Swift error) are
/// already localized prose and carry no detail; core errors crossing the bridge
/// arrive as typed reasons and render their generic per-category line here.
public struct DisplayError: Equatable {
    private static let detailExcerptLength = 400

    public let line: String
    public let detail: String?

    /// The opening portion rendered inline. Copy actions continue to use
    /// `detail`, which retains the complete diagnostic.
    public var detailExcerpt: String? {
        guard let detail else { return nil }
        let excerpt = detail.prefix(Self.detailExcerptLength)
        return excerpt.endIndex == detail.endIndex ? detail : "\(excerpt)…"
    }

    /// A UI-originated error: prose the UI already localized, with no opaque
    /// detail to disclose.
    public init(line: String) {
        self.line = line
        self.detail = nil
    }

    private init(line: String, detail: String?) {
        self.line = line
        self.detail = detail
    }

    /// Put the operation or object that failed before the existing localized
    /// line without discarding the opaque diagnostic offered by "Copy Details".
    public func addingContext(_ context: String) -> DisplayError {
        DisplayError(line: "\(context): \(line)", detail: detail)
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
        self.detail = failure.detail
    }

    /// Any error on its way to a human.
    ///
    /// A failure that crossed the bridge renders core's keyed line and keeps its
    /// opaque detail for the copy button. Anything else is Swift-origin prose and
    /// passes through unchanged — which is why a `catch` can be routed here
    /// without first proving what it can throw.
    ///
    /// Never reach for `localizedDescription` on the way to a human: uniffi gives
    /// `BridgeError` a `String(reflecting:)` description, so it renders as a raw
    /// Rust enum dump — untranslated, with the opaque detail welded into the line
    /// instead of offered to the copy button.
    ///
    /// Fails when core says the failure has no line to show — a cancellation. A
    /// Swift-origin error always has one: it is prose the UI wrote.
    public init?(_ error: any Error) {
        guard let failure = error as? LocalizedFailure else {
            self.line = error.localizedDescription
            self.detail = nil
            return
        }
        guard let line = failure.localizedLine else { return nil }
        self.line = line
        self.detail = failure.detail
    }
}

extension Error {
    /// The line to show a human, for the places that hold a plain `String?` rather
    /// than a `DisplayError`. `nil` means there is nothing to show. Prefer
    /// `DisplayError(error)` where the type allows it — that one keeps the opaque
    /// detail for the "Copy Details" button.
    ///
    /// This is `localizedDescription`'s job, and `localizedDescription` cannot do
    /// it: uniffi declares `errorDescription` on `BridgeError` itself as
    /// `String(reflecting: self)`, an extension cannot override it, and in a
    /// `catch` the static type is `any Error`, so the debug dump always wins.
    public var displayLine: String? {
        DisplayError(self)?.line
    }
}
