using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// UI-side formatting for device membership: the localized role label the member
/// list renders.
/// </summary>
internal static class MemberFormat
{
    /// <summary>The localized display name for a generated member role.</summary>
    internal static string RoleLabel(BridgeMemberRole role) => role switch
    {
        BridgeMemberRole.Owner => Loc.Chrome("members.role.owner"),
        BridgeMemberRole.Member => Loc.Chrome("members.role.member"),
        BridgeMemberRole.Follower => Loc.Chrome("members.role.follower"),
        _ => throw new ArgumentOutOfRangeException(nameof(role), role, "Unknown member role"),
    };
}

/// <summary>
/// This device's join-request code and the fingerprint it encodes, to hand to an
/// existing member for approval.
/// </summary>
public sealed class JoinRequest
{
    public string Code { get; set; } = string.Empty;

    /// <summary>Short display identity — the first 8 characters of the pubkey.</summary>
    public string Fingerprint { get; set; } = string.Empty;
}

/// <summary>
/// What a join-request code decodes to: the joining device's public key, its
/// fingerprint, and an optional contact email it included.
/// </summary>
public sealed class JoinRequestInfo
{
    public string Pubkey { get; set; } = string.Empty;

    /// <summary>Short display identity — the first 8 characters of the pubkey.</summary>
    public string Fingerprint { get; set; } = string.Empty;

    public string? Email { get; set; }
}

/// <summary>What an invite code decodes to.</summary>
public sealed class InviteCodeInfo
{
    public string LibraryId { get; set; } = string.Empty;
    public string LibraryName { get; set; } = string.Empty;
    public string OwnerPubkey { get; set; } = string.Empty;

    /// <summary>
    /// Short display identity of the library owner — the first 8 characters of
    /// the owner pubkey.
    /// </summary>
    public string OwnerFingerprint { get; set; } = string.Empty;

    public string Provider { get; set; } = string.Empty;
    public bool NeedsOauth { get; set; }
}
