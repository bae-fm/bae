import Foundation

extension BridgeRemoteCover {
    /// What the picker calls this image; see `coverLabelText`.
    public var labelText: String {
        coverLabelText(sourceName: sourceName, label: label)
    }
}

/// What the picker calls a catalog's image, worded from the `Core` messages:
/// "Cover Art Archive", "Cover Art Archive (album)" for the album's release
/// group, or the parts of a gallery entry (service, catalog reference,
/// number, what it shows, the catalog's comment) between the language's
/// separator.
public func coverLabelText(sourceName: String, label: BridgeCoverLabel)
    -> String
{
    switch label {
    case .lead(let releaseGroup):
        guard releaseGroup else { return sourceName }
        return localizedCoreString(
            "core.cover.label.release_group",
            sourceName
        )
    case .gallery(let reference, let position, let types, let comment):
        var parts = [sourceName]
        if let reference {
            parts.append("[\(reference)]")
        }
        parts.append(Int(position).formatted(.number.grouping(.never)))
        if !types.isEmpty {
            let words = types.map {
                localizedCoreString(bridgeCoverImageTypeKey(imageType: $0))
            }
            parts.append(ListFormatter.localizedString(byJoining: words))
        }
        if !comment.isEmpty {
            parts.append(comment)
        }
        return parts.joined(
            separator: localizedCoreString("core.cover.label.separator")
        )
    }
}
