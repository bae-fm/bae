using System.ComponentModel;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>One album in the library grid.</summary>
public sealed class Album : INotifyPropertyChanged
{
    private readonly string _id;
    private readonly string _title;
    private readonly string _artist;
    private readonly CoverImage.Binding _cover;

    internal Album(BridgeAlbumSearchResult album) : this(album.Id, album.Title, album.ArtistName, album.Cover)
    {
    }

    internal Album(BridgeAlbum album) : this(album.Id, album.Title, album.ArtistNames, album.Cover)
    {
    }

    private Album(string id, string title, string artist, BridgeImageRef? cover)
    {
        _id = id;
        _title = title;
        _artist = artist;
        _cover = new CoverImage.Binding(cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Id => _id;
    public string Title => _title;
    public string Artist => _artist;

    internal void AttachCover(LibraryHandle handle, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(handle, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
}
