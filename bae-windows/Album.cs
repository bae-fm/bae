using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

/// <summary>
/// One album in the library grid. Deserialized from the FFI's JSON array
/// (<c>{id, title, artist, cover_path}</c>, snake_case). <see cref="Cover"/> is
/// computed from <see cref="CoverPath"/> for the grid's image — null when the
/// album has no cover cached on disk, which the template renders as a blank tile.
///
/// <see cref="CoverPath"/> is the FFI's cache-bustable cover identifier
/// (<c>&lt;path&gt;#v=&lt;mtime&gt;</c>): its version changes when the cover does,
/// so <c>x:Bind Cover</c> re-evaluates and <see cref="CoverImage"/> reloads the
/// current bytes.
/// </summary>
public sealed class Album
{
    public string Id { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;
    public string? CoverPath { get; set; }

    public ImageSource? Cover => CoverImage.Load(CoverPath);
}
