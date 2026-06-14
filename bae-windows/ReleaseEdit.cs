using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// The metadata editor's raw form, mirroring <c>bae_core::import::RawReleaseEdit</c>
/// across the FFI. Seeded by <c>bae_release_edit_seed</c> and round-tripped back
/// to <c>bae_apply_release_edit</c>, which shapes (validates) and writes it. The
/// form edits the album title, artists, pressing, and each track's title, artist,
/// side, and number.
/// </summary>
public sealed class ReleaseEdit
{
    public string AlbumTitle { get; set; } = string.Empty;

    /// <summary>Comma-separated artist text in positional order.</summary>
    public string AlbumArtistText { get; set; } = string.Empty;

    public PressingEdit Pressing { get; set; } = new();
    public List<TrackEdit> Tracks { get; set; } = new();
}

/// <summary>Raw pressing fields as text; empty means "not set".</summary>
public sealed class PressingEdit
{
    public string Year { get; set; } = string.Empty;
    public string Format { get; set; } = string.Empty;
    public string Label { get; set; } = string.Empty;
    public string CatalogNumber { get; set; } = string.Empty;
    public string Country { get; set; } = string.Empty;
    public string Barcode { get; set; } = string.Empty;
}

/// <summary>One raw track row; preserved verbatim by the album-level editor.</summary>
public sealed class TrackEdit
{
    public string Id { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string ArtistText { get; set; } = string.Empty;
    public int Side { get; set; }
    public int? TrackNumber { get; set; }
}
