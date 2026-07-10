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
    private bool _isSelected;

    internal Album(BridgeAlbumSearchResult album) : this(album.Id, album.Title, album.ArtistName, album.Year, album.Cover)
    {
    }

    internal Album(BridgeAlbum album) : this(album.Id, album.Title, album.ArtistNames, album.Year, album.Cover)
    {
        PrimaryReleaseId = album.PrimaryReleaseId;
    }

    private Album(string id, string title, string artist, int? year, BridgeImageRef? cover)
    {
        _id = id;
        _title = title;
        _artist = artist;
        Year = year;
        _cover = new CoverImage.Binding(cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Id => _id;
    public string Title => _title;
    public string Artist => _artist;
    public int? Year { get; }

    /// <summary>The album's canonical release — the default play/queue target.
    /// Null for search-result albums, whose bridge type doesn't carry it.</summary>
    internal string? PrimaryReleaseId { get; }

    internal void AttachCover(LibraryHandle handle, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(handle, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;

    /// <summary>Whether the grid's multi-selection contains this album. The
    /// window syncs this from <see cref="AlbumGridSelectionModel"/> after every
    /// selection mutation; the card's tint binds to it OneWay.</summary>
    public bool IsSelected
    {
        get => _isSelected;
        set
        {
            if (_isSelected == value)
            {
                return;
            }
            _isSelected = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsSelected)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(SelectionTintOpacity)));
        }
    }

    /// <summary>The selection tint's opacity, precomputed so the card's XAML
    /// binds it directly — a compiled binding in Window XAML can't resolve a
    /// StaticResource converter (the generated lookup passes the Window, which
    /// isn't a FrameworkElement in WinUI 3).</summary>
    public double SelectionTintOpacity => _isSelected ? 1.0 : 0.0;
}
