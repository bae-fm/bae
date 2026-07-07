namespace Bae.Windows;

/// <summary>
/// UI-side formatting for device membership: the localized role label the member
/// list renders.
/// </summary>
internal static class MemberFormat
{
    /// <summary>The localized display name for a wire role tag.</summary>
    internal static string RoleLabel(string role) => role switch
    {
        "owner" => Loc.Chrome("members.role.owner"),
        "member" => Loc.Chrome("members.role.member"),
        "follower" => Loc.Chrome("members.role.follower"),
        _ => role,
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

/// <summary>
/// The library's membership (from <c>bae_get_members</c>): its devices and
/// whether the running device is an owner (the gate for inviting and removing).
/// </summary>
public sealed class Membership
{
    public List<Member> Members { get; set; } = [];
    public bool SelfIsOwner { get; set; }
}

/// <summary>One device in the library (from <c>bae_get_members</c>).</summary>
public sealed class Member
{
    public string Pubkey { get; set; } = string.Empty;

    /// <summary>Lowercase wire role: "owner" / "member" / "follower".</summary>
    public string Role { get; set; } = string.Empty;

    /// <summary>True for the device this app is running on.</summary>
    public bool IsSelf { get; set; }

    /// <summary>Short display identity — the first 8 characters of the pubkey.</summary>
    public string Fingerprint { get; set; } = string.Empty;

    /// <summary>Whether the running device may remove this one (owner-only, never self).</summary>
    public bool CanRemove { get; set; }
}
