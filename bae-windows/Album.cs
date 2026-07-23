using System.ComponentModel;
using System.Globalization;
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
    private bool _isExpanded;

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

    /// <summary>The year for the tile's year line, or an empty string when unknown
    /// (the line keeps its height so tiles stay uniform). Plain digits, no group
    /// separator — a four-digit year formats the same in every culture.</summary>
    public string YearText => Year?.ToString(CultureInfo.CurrentCulture) ?? string.Empty;

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

    /// <summary>Whether this album's inline detail expansion is open — the one
    /// album (at most) whose card shows the accent ring and hosts the expansion
    /// panel below its grid row. Distinct from <see cref="IsSelected"/>: the
    /// expansion is a single-album, plain-click concern; multi-select is
    /// modifier-driven. Opening an expansion clears the multi-selection.</summary>
    public bool IsExpanded
    {
        get => _isExpanded;
        set
        {
            if (_isExpanded == value)
            {
                return;
            }
            _isExpanded = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsExpanded)));
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(ExpansionRingOpacity)));
        }
    }

    /// <summary>The accent ring's opacity on the expanded card's cover, toggled
    /// (not shown/hidden) so a card the grid recycles never re-measures — the
    /// same discipline as <see cref="SelectionTintOpacity"/>.</summary>
    public double ExpansionRingOpacity => _isExpanded ? 1.0 : 0.0;
}
