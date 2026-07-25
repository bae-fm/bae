using uniffi.bae_bridge;

namespace Bae.Desktop;

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
