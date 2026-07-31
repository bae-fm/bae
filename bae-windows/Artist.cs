using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

public sealed class ArtistSummary : INotifyPropertyChanged
{
    private readonly BridgeArtistSummary _artist;
    private readonly ImageBinding _cover;

    internal ArtistSummary(BridgeArtistSummary artist)
    {
        _artist = artist;
        _cover = new ImageBinding(ImageContent.ForLibraryImage(artist.Image), ImageWidths.Row);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string ArtistId => _artist.ArtistId;
    public string Name => _artist.Name;
    public long AlbumCount => _artist.AlbumCount;

    internal void AttachCover(ImageStore imageStore, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(imageStore, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
    public string AlbumCountText => Loc.Chrome("album.count", "count", Loc.Number(AlbumCount));
}

public sealed class ArtistDetail
{
    internal ArtistDetail(BridgeArtistDetail detail)
    {
        Artist = new ArtistSummary(detail.Artist);
        Albums = detail.Albums.Select(album => new Album(album)).ToList();
    }

    public ArtistSummary Artist { get; }
    public List<Album> Albums { get; }
}
