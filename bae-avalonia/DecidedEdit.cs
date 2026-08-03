using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// A candidate's decided identity mapped onto the pane's edit shape — what the
/// pick command and the selection query both come back with, so a fresh launch
/// renders exactly what the click rendered.
/// </summary>
internal sealed class DecidedEdit
{
    /// <summary>The picked release, or null for a folder read as its own
    /// tags.</summary>
    public (BridgeMetadataSource Source, string ReleaseId)? Release { get; set; }

    /// <summary>The seeded edit either identity stands for.</summary>
    public PrefetchedEdit Edit { get; set; } = new();
}
