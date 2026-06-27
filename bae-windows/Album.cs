using System;
using System.Text.Json.Serialization;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

/// <summary>
/// One album in the library grid. Deserialized from the FFI's JSON array
/// (<c>{id, title, artist, cover}</c>, snake_case). <see cref="Cover"/> is the
/// decoded image for the grid tile — null when the album has no cover, which the
/// template renders as a blank tile.
///
/// <see cref="CoverRef"/> is the FFI's image reference (id + content version): the
/// version moves when the cover changes, so <see cref="CoverImage"/> keys its
/// decode cache on it and a changed cover re-decodes. <see cref="Handle"/> is
/// injected after deserialize (the wire shape carries no handle) so the tile can
/// fetch the cover bytes by id.
/// </summary>
public sealed class Album
{
    public string Id { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;

    [JsonPropertyName("cover")]
    public ImageRef? CoverRef { get; set; }

    [JsonIgnore]
    public IntPtr Handle { get; set; }

    public ImageSource? Cover => CoverImage.LoadByImageRef(Handle, CoverRef);
}

/// <summary>
/// A reference to a library image: its id plus a content version that moves when
/// the image changes. Mirrors the FFI's <c>FfiImageRef</c> (<c>{id, version}</c>).
/// The version is the cache key for the decoded bitmap (see <see cref="CoverImage"/>).
/// </summary>
public sealed class ImageRef
{
    public string Id { get; set; } = string.Empty;
    public string Version { get; set; } = string.Empty;
}
