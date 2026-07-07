using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>One album in the library grid.</summary>
public sealed class Album
{
    private readonly BridgeAlbumSearchResult _album;

    public Album(BridgeAlbumSearchResult album)
    {
        _album = album;
    }

    public string Id => _album.Id;
    public string Title => _album.Title;
    public string Artist => _album.ArtistName;

    internal AppHandle? Handle { get; set; }

    public ImageSource? Cover => CoverImage.LoadByImageRef(Handle, _album.Cover);
}
