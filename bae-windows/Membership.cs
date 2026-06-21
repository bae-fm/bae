namespace Bae.Windows;

/// <summary>
/// UI-side formatting for device membership: the public-key fingerprint and the
/// localized role label the member list renders.
/// </summary>
internal static class MemberFormat
{
    /// <summary>
    /// A device's short fingerprint — the first eight hex characters of its
    /// public key — shown on both the joining and approving device so the user
    /// can confirm they're pairing the right one.
    /// </summary>
    internal static string Fingerprint(string pubkey) =>
        pubkey.Length <= 8 ? pubkey : pubkey[..8];

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
/// What a join-request code decodes to (from <c>bae_decode_join_request</c>):
/// the joining device's public key and an optional contact email it included.
/// </summary>
public sealed class JoinRequestInfo
{
    public string Pubkey { get; set; } = string.Empty;
    public string? Email { get; set; }
}

/// <summary>What an invite code decodes to (from <c>bae_decode_invite_code</c>).</summary>
public sealed class InviteCodeInfo
{
    public string LibraryId { get; set; } = string.Empty;
    public string LibraryName { get; set; } = string.Empty;
    public string OwnerPubkey { get; set; } = string.Empty;
    public string Provider { get; set; } = string.Empty;
    public bool NeedsOauth { get; set; }
}

/// <summary>One device in the library (from <c>bae_get_members</c>).</summary>
public sealed class Member
{
    public string Pubkey { get; set; } = string.Empty;

    /// <summary>Lowercase wire role: "owner" / "member" / "follower".</summary>
    public string Role { get; set; } = string.Empty;

    /// <summary>True for the device this app is running on.</summary>
    public bool IsSelf { get; set; }
}
