namespace Bae.Windows;

/// <summary>
/// One remote cover-art candidate, deserialized from the FFI's
/// <c>bae_fetch_remote_covers</c> JSON. The picker loads <see cref="ThumbnailUrl"/>
/// for the grid and passes <see cref="Url"/> + <see cref="Source"/> back to
/// <c>bae_change_cover</c> when the user selects it.
/// </summary>
public sealed class RemoteCover
{
    public string Url { get; set; } = string.Empty;
    public string ThumbnailUrl { get; set; } = string.Empty;
    public string Label { get; set; } = string.Empty;

    /// <summary>The wire source name ("musicbrainz" / "discogs").</summary>
    public string Source { get; set; } = string.Empty;
}
