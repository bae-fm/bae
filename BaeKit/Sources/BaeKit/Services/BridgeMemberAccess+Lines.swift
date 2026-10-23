import Foundation

extension BridgeMemberAccess {
    /// The S3 key's id as a member or invite shows it, or nil for an account.
    public var keyLine: String? {
        guard case .s3AccessKey(let accessKeyId) = self else { return nil }
        return String(
            format: localizedCoreString("core.access_key.id"),
            locale: Locale.current,
            accessKeyId
        )
    }

    /// Below an S3 invite's code: if nobody joins with it, the person deletes
    /// its key in the provider's console. Nil for an account invite.
    public var unusedInviteLine: String? {
        guard case .s3AccessKey(let accessKeyId) = self else { return nil }
        return String(
            format: localizedCoreString("core.access_key.unused_invite"),
            locale: Locale.current,
            accessKeyId
        )
    }

    /// What removing a member or cancelling an invite leaves the person to
    /// do: delete its S3 key in the provider's console, since bae keeps no
    /// record of it. Nil for an account.
    public var forgetLine: String? {
        guard case .s3AccessKey(let accessKeyId) = self else { return nil }
        return String(
            format: localizedCoreString("core.access_key.forget"),
            locale: Locale.current,
            accessKeyId
        )
    }
}
