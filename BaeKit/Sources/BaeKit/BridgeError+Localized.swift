import Foundation

extension BridgeErrorCategory {
    /// The user-facing line for this category, resolved from the generated
    /// `Core` string table via the key bae-library owns, with the counts and
    /// disc numbers a category carries formatted for the current locale. The underlying Rust error chain travels separately as
    /// opaque `detail` and is never translated.
    public var localizedLine: String {
        let line = localizedCoreString(bridgeErrorCategoryKey(category: self))
        switch self {
        case .metadataTrackCount(let folderTracks, let releaseTracks):
            return String(
                format: line,
                locale: Locale.current,
                Int(folderTracks),
                Int(releaseTracks)
            )
        case .tiedMediums(let mediums):
            let discs = ListFormatter.localizedString(
                byJoining: mediums.map {
                    NumberFormatter.localizedString(
                        from: NSNumber(value: $0),
                        number: .none
                    )
                }
            )
            return String(format: line, locale: Locale.current, discs)
        case .airPlayBackOff(let seconds):
            // The wait the receiver asked for, as a length of time in the
            // current locale ("30 seconds", "2 minutes").
            let wait = Duration.seconds(Int64(clamping: seconds))
                .formatted(
                    .units(allowed: [.hours, .minutes, .seconds], width: .wide)
                )
            return String(format: line, locale: Locale.current, wait)
        default:
            return line
        }
    }
}

extension BridgeEntityKind {
    /// The "… not found" line for this entity, resolved from the generated
    /// `Core` string table via the key bae-library owns.
    public var notFoundLine: String {
        localizedCoreString(bridgeEntityNotFoundKey(entity: self))
    }
}

extension BridgeError {
    /// The localized, user-facing line for this error, or `nil` when it has none
    /// to show — core's answer, not this app's. A cancellation is the user's own
    /// doing and says nothing back to them.
    ///
    /// `nil`, never `""`: an empty string is not "nothing", it is a line that
    /// happens to be blank, and it used to open the error alert with an empty
    /// message.
    public var localizedLine: String? {
        guard let key = bridgeErrorLineKey(error: self) else { return nil }
        // A diagnostic's key is its category's, whose line also takes the
        // counts the category carries.
        if case .Diagnostic(let category, _) = self {
            return category.localizedLine
        }
        return localizedCoreString(key)
    }

    /// The opaque Rust error chain to show under the line and offer to copy,
    /// never translated, or `nil` when core says there is none to show: a
    /// `NotFound` or `Cancelled` carries no detail, and a category whose line
    /// names the whole fault shows that line alone rather than the same
    /// sentence again in English. Core's answer, not this app's.
    public var shownDetail: String? {
        bridgeErrorShownDetail(error: self)
    }

    /// The library whose unfinished join or restore on this device can't be
    /// read, when this error says so: listing the joins and restores fails
    /// with it until the app discards that installation. Core's answer.
    public var unreadableInstallationLibraryId: String? {
        guard case .Diagnostic(let category, _) = self,
            case .unfinishedInstallationUnreadable(let libraryId) = category
        else { return nil }
        return libraryId
    }

    /// Whether core says making the same call again can't get past this
    /// error: a damaged database, a bug, an edit to fix, a call only another
    /// call answers. Core's answer, not this app's.
    public var retryRuledOut: Bool {
        !bridgeErrorRetryCanHelp(error: self)
    }

    /// Whether core says resetting the library from the cloud replaces what
    /// failed: the failure is in this device's copy of the library's data.
    /// Core's answer, not this app's.
    public var resetFromCloudReplaces: Bool {
        bridgeErrorResetFromCloudReplaces(error: self)
    }
}

/// A bridge error's `localizedDescription` is core's keyed line in the current
/// locale. The generated type has no `LocalizedError` conformance of its own:
/// `bae-bridge/uniffi.toml` leaves out uniffi's, whose description is
/// `String(reflecting: self)`, a dump of the Rust value.
///
/// A cancellation has no line, so Foundation substitutes its generic sentence
/// for it. Whatever shows an error to a person goes through `DisplayError`,
/// which shows nothing for a cancellation and keeps a diagnostic's detail for
/// the copy button.
extension BridgeError: LocalizedError {
    public var errorDescription: String? { localizedLine }
}

extension BridgePlaybackErrorReason {
    /// The localized, user-facing line for this playback failure, or `nil` when
    /// there is none. The actionable cases, where this device can't reach the
    /// audio, resolve their own keyed line; everything else renders through the
    /// shared `BridgeError` path, which is also where "no line at all" comes
    /// from.
    public var localizedLine: String? {
        switch self {
        case .syncDisconnected, .onOtherDevice, .originalMissing,
            .audioOutput:
            return bridgePlaybackErrorReasonLine(reason: self)?.localizedLine
        case .diagnostic(let error):
            return error.localizedLine
        }
    }

    /// The detail to show under the line. Only the diagnostic arm has one:
    /// the composed `BridgeError`'s shown detail.
    public var shownDetail: String? {
        switch self {
        case .diagnostic(let error):
            return error.shownDetail
        case .syncDisconnected, .onOtherDevice, .originalMissing,
            .audioOutput:
            return nil
        }
    }

    /// Only the diagnostic arm carries core's answer, its error's; core
    /// keys the others without saying whether playing again can pass.
    public var retryRuledOut: Bool {
        switch self {
        case .diagnostic(let error):
            return error.retryRuledOut
        case .syncDisconnected, .onOtherDevice, .originalMissing,
            .audioOutput:
            return false
        }
    }

    /// Only the diagnostic arm carries core's answer, its error's.
    public var resetFromCloudReplaces: Bool {
        switch self {
        case .diagnostic(let error):
            return error.resetFromCloudReplaces
        case .syncDisconnected, .onOtherDevice, .originalMissing,
            .audioOutput:
            return false
        }
    }
}
