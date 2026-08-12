using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// A candidate's decided identity mapped onto the pane's edit shape — what the
/// pick command and the selection query both come back with, so a fresh launch
/// renders exactly what the click rendered.
/// </summary>
internal sealed class DecidedEdit
{
    /// <summary>The picked release and how far the claim on it reaches, or
    /// null for a folder read as its own tags. The level rides here because a
    /// re-pick has to send it back, and a folder read as its own tags has no
    /// claim to carry.</summary>
    public (
        BridgeMetadataSource Source,
        string ReleaseId,
        string? SourceGroupId,
        BridgeClaimLevel Claim)? Release { get; set; }

    /// <summary>The seeded edit either identity stands for.</summary>
    public PrefetchedEdit Edit { get; set; } = new();
}
