import Foundation

extension BridgeReleaseStorageAction {
    /// Present-continuous progress verb ("Pinning for offline"), localized
    /// against the generated `Core` table. bae-core decides the action; this is
    /// the UI's locale rendering of it.
    var transferProgressVerb: String {
        NSLocalizedString(
            bridgeTransferActionKey(action: self),
            tableName: "Core",
            bundle: .main,
            comment: ""
        )
    }
}

/// The localized transfer-progress line, e.g. "Pinning for offline — 3 of 12
/// files · 42%". Before per-file progress is known (`fileNo`/`total` nil), it's
/// just the verb. bae-core owns the action and the counts; the UI composes and
/// formats them for the current locale.
func transferProgressLabel(
    action: BridgeReleaseStorageAction,
    fileNo: UInt32?,
    total: UInt32?,
    percent: UInt8
) -> String {
    guard let fileNo, let total else { return action.transferProgressVerb }
    let files = String(
        format: NSLocalizedString(
            "core.transfer.files",
            tableName: "Core",
            bundle: .main,
            comment: ""
        ),
        Int(fileNo),
        Int(total)
    )
    let pct = (Double(percent) / 100).formatted(.percent)
    return "\(action.transferProgressVerb) — \(files) · \(pct)"
}
