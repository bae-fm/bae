using System.ComponentModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>One album in the library grid.</summary>
public sealed class Album : INotifyPropertyChanged
{
    private readonly BridgeAlbumSearchResult _album;
    private readonly CoverImage.Binding _cover;

    public Album(BridgeAlbumSearchResult album)
    {
        _album = album;
        _cover = new CoverImage.Binding(album.Cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Id => _album.Id;
    public string Title => _album.Title;
    public string Artist => _album.ArtistName;

    internal void AttachCover(LibraryHandle handle, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(handle, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
}
