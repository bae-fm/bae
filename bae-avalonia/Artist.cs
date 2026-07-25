using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using Avalonia.Threading;
using Avalonia.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Desktop;

public sealed class ArtistSummary : INotifyPropertyChanged
{
    private readonly BridgeArtistSummary _artist;
    private readonly CoverImage.Binding _cover;

    internal ArtistSummary(BridgeArtistSummary artist)
    {
        _artist = artist;
        _cover = new CoverImage.Binding(artist.Image);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string ArtistId => _artist.ArtistId;
    public string Name => _artist.Name;
    public long AlbumCount => _artist.AlbumCount;

    internal void AttachCover(MediaPathsService mediaPaths, Dispatcher dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    public Bitmap? Cover => _cover.Source;
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
