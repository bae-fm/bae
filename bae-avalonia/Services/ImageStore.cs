using System;
using System.IO;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Media.Imaging;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>What an image slot shows, and therefore where its bytes come from.
/// Every image in the app is one of these five; a view names the content and
/// renders whatever <see cref="ImageStore"/> hands back.</summary>
internal abstract record ImageContent
{
    /// <summary>A curated library image — a release cover or an artist portrait —
    /// read by its versioned reference.</summary>
    internal sealed record LibraryImage(BridgeImageRef Image) : ImageContent;

    /// <summary>One slot of a release's image strip: its cover, or one of the
    /// release's own image files. Core dispatches the read on the source, so the
    /// UI never picks the byte source itself.</summary>
    internal sealed record ReleaseImage(string ReleaseId, BridgeGallerySource Source) : ImageContent;

    /// <summary>Provider art that isn't in the library. Fetched through core,
    /// which owns every socket the app opens.</summary>
    internal sealed record Remote(string Url) : ImageContent;

    /// <summary>A file on disk the user is previewing before it enters the
    /// library — an import candidate's cover or folder image.</summary>
    internal sealed record LocalFile(string Path) : ImageContent;

    /// <summary>Bytes already in hand. Decoded on demand and never cached: the
    /// caller holds the only identity these bytes have.</summary>
    internal sealed record Bytes(byte[] Data) : ImageContent;

    /// <summary>The content for a library image reference, or null when the
    /// subject has no image.</summary>
    internal static ImageContent? ForLibraryImage(BridgeImageRef? image) =>
        image is null ? null : new LibraryImage(image);

    /// <summary>The content for a cover the import flow offers: a candidate file
    /// on disk, or provider art at a URL.</summary>
    internal static ImageContent ForCoverSource(BridgeCoverImageSource source) => source switch
    {
        BridgeCoverImageSource.Local local => new LocalFile(local.Path),
        BridgeCoverImageSource.Remote remote => new Remote(remote.Url),
        _ => throw new ArgumentOutOfRangeException(nameof(source)),
    };

    /// <summary>Human-readable description for failure logs.</summary>
    internal string Description => this switch
    {
        LibraryImage library => $"library image: {library.Image.ImageType} {library.Image.Id}",
        ReleaseImage release => $"release image: {release.ReleaseId}",
        Remote remote => $"remote image: {remote.Url}",
        LocalFile local => $"image at path: {local.Path}",
        Bytes bytes => $"in-memory image: {bytes.Data.Length} bytes",
        _ => "image",
    };

    internal ImageBucket Bucket => this switch
    {
        LibraryImage => ImageBucket.LibraryImage,
        ReleaseImage => ImageBucket.ReleaseImage,
        Remote => ImageBucket.Remote,
        // Raw bytes are never cached, but every content still names a bucket so
        // the lookup needs no second "is this cacheable" branch — its key is
        // null, which is what keeps it out.
        _ => ImageBucket.LocalFile,
    };
}

/// <summary>
/// The app's image pipeline: bytes → decode at the slot's pixel width → bounded
/// decoded cache → synchronous read for a control that is rebinding. Views hold
/// no fetch, cache, or decode logic of their own, and none of them opens a
/// socket — provider art is read through core like everything else.
///
/// Decoding runs off the UI thread and the result is applied on the dispatcher.
/// Cache identity is <see cref="ImageTokens"/>'; the budgets and eviction are
/// <see cref="ImageCache{TImage}"/>'s.
/// </summary>
internal sealed class ImageStore
{
    /// <summary>Bytes of a curated library image, or null when no such image
    /// exists.</summary>
    public Func<BridgeImageRef, byte[]?> FetchLibraryImageBytes { get; init; }
        = _ => throw new InvalidOperationException("ImageStore stub: FetchLibraryImageBytes not wired");

    /// <summary>Bytes of one of a release's image-strip slots, downloaded from
    /// the release's cloud home (and decrypted) when it isn't on disk here.</summary>
    public Func<string, BridgeGallerySource, byte[]?> FetchReleaseImageBytes { get; init; }
        = (_, _) => throw new InvalidOperationException("ImageStore stub: FetchReleaseImageBytes not wired");

    /// <summary>Bytes of provider art at a URL, with the validator identifying
    /// them.</summary>
    public Func<string, BridgeRemoteImage?> FetchRemoteImage { get; init; }
        = _ => throw new InvalidOperationException("ImageStore stub: FetchRemoteImage not wired");

    private readonly ImageCache<Bitmap> _cache = new(DecodedByteCost);

    /// <summary>Wire every read through the open session's current handle.</summary>
    public static ImageStore FromSession(SessionStore session) => new()
    {
        FetchLibraryImageBytes = image =>
            session.WithCurrentHandle(handle => NativeBae.LibraryImageBytes(handle, image)).Result,
        FetchReleaseImageBytes = (releaseId, source) =>
            session.WithCurrentHandle(handle => NativeBae.ReleaseImageBytes(handle, releaseId, source)).Result,
        FetchRemoteImage = url =>
            session.WithCurrentHandle(handle => NativeBae.RemoteImage(handle, url)).Result,
    };

    /// <summary>The already-decoded image for this content at this width, or null
    /// when the store doesn't hold one. Synchronous, so a control can paint real
    /// art the moment it rebinds instead of blanking first. The only I/O is the
    /// stat a local file's modification date needs.</summary>
    internal Bitmap? Cached(ImageContent content, int pixelWidth)
    {
        var key = CacheKey(content, pixelWidth);
        return key is null ? null : _cache.Get(content.Bucket, key);
    }

    /// <summary>
    /// The decoded image for this content at this width: the cached decode when
    /// there is one, otherwise fetch the bytes, decode off the UI thread, and
    /// store the result. Null when no such image exists, or when the bytes can't
    /// be read or decoded (both logged).
    /// </summary>
    internal async Task<Bitmap?> LoadAsync(ImageContent content, int pixelWidth)
    {
        var key = CacheKey(content, pixelWidth);
        if (key is not null && _cache.Get(content.Bucket, key) is { } cached)
        {
            return cached;
        }

        var bitmap = await Task.Run(() =>
        {
            var bytes = ReadBytes(content);
            return bytes is null ? null : Decode(bytes, pixelWidth);
        }).ConfigureAwait(true);

        // A key is absent only for raw bytes, whose identity the caller holds —
        // there is nothing to pin a cache entry to.
        if (bitmap is not null && key is not null)
        {
            _cache.Store(content.Bucket, key, bitmap);
            if (content is ImageContent.Remote remote)
            {
                _cache.RecordRemoteKey(remote.Url, key);
            }
        }

        return bitmap;
    }

    /// <summary>
    /// Bind an image control to this content, loaded off the UI thread and applied
    /// when it lands. Recycling-safe: the requested key is stamped on the
    /// control's <see cref="Control.Tag"/> at bind time and the reply is applied
    /// only while that stamp still matches, so a row recycled to other content
    /// mid-load drops the stale reply instead of painting the wrong art. A cached
    /// decode is applied straight away, which is what keeps a scrolled-back row
    /// from blanking.
    /// </summary>
    internal void Bind(Image control, ImageContent? content, int pixelWidth)
    {
        var key = content is null ? null : CacheKey(content, pixelWidth);
        control.Tag = key;
        control.Source = null;
        if (content is null)
        {
            return;
        }

        if (key is not null && _cache.Get(content.Bucket, key) is { } cached)
        {
            control.Source = cached;
            return;
        }

        var dispatcher = Dispatcher.UIThread;
        _ = LoadAsync(content, pixelWidth).ContinueWith(
            task =>
            {
                var bitmap = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                if (bitmap is null)
                {
                    return;
                }

                dispatcher.Post(() =>
                {
                    if ((control.Tag as string) == key)
                    {
                        control.Source = bitmap;
                    }
                });
            },
            TaskScheduler.Default);
    }

    /// <summary>Raw bytes for this content, with no decode and no caching — the
    /// lightbox decodes them at native resolution itself. Null when no such image
    /// exists (logged).</summary>
    internal byte[]? ReadBytes(ImageContent content)
    {
        try
        {
            switch (content)
            {
                case ImageContent.LibraryImage library:
                    return FetchLibraryImageBytes(library.Image);
                case ImageContent.ReleaseImage release:
                    return FetchReleaseImageBytes(release.ReleaseId, release.Source);
                case ImageContent.Remote remote:
                    var fetched = FetchRemoteImage(remote.Url);
                    if (fetched is null)
                    {
                        return null;
                    }

                    // Core's byte cache decides when a URL is re-read; when the
                    // bytes come back different, the decodes taken from the old
                    // ones are stale at every size, not just the one being loaded.
                    // This is the only moment the store learns they moved — a size
                    // it already holds is served from the cache and asks nothing.
                    _cache.AdoptRemoteValidator(remote.Url, fetched.Validator);
                    return fetched.Bytes;
                case ImageContent.LocalFile local:
                    return File.ReadAllBytes(local.Path);
                case ImageContent.Bytes bytes:
                    return bytes.Data;
                default:
                    throw new ArgumentOutOfRangeException(nameof(content));
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning($"Failed to read {content.Description}: {exception.Message}");
            return null;
        }
    }

    private string? CacheKey(ImageContent content, int pixelWidth)
    {
        var token = Token(content);
        return token is null ? null : ImageTokens.Key(token, pixelWidth);
    }

    private static string? Token(ImageContent content) => content switch
    {
        ImageContent.LibraryImage library => LibraryToken(library.Image),
        // A cover slot IS the release's curated cover; its version moves whenever
        // the bytes do.
        ImageContent.ReleaseImage { Source: BridgeGallerySource.Cover cover } => LibraryToken(cover.Image),
        ImageContent.ReleaseImage { Source: BridgeGallerySource.ReleaseFile file } =>
            ImageTokens.ReleaseFile(file.FileId),
        ImageContent.Remote remote => ImageTokens.Remote(remote.Url),
        ImageContent.LocalFile local => ImageTokens.LocalFile(local.Path),
        _ => null,
    };

    private static string LibraryToken(BridgeImageRef image) =>
        ImageTokens.Library(image.ImageType.ToString(), image.Id, image.Version);

    /// <summary>Decode bytes to at most <paramref name="pixelWidth"/> across, so a
    /// 3000px scan never occupies a 44px row's worth of budget. Null when the
    /// bytes are corrupt or an unsupported format.</summary>
    private static Bitmap? Decode(byte[] bytes, int pixelWidth)
    {
        try
        {
            using var stream = new MemoryStream(bytes);
            return Bitmap.DecodeToWidth(stream, pixelWidth);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning($"Failed to decode an image: {exception.Message}");
            return null;
        }
    }

    private static int DecodedByteCost(Bitmap bitmap) =>
        Math.Max(1, bitmap.PixelSize.Width * bitmap.PixelSize.Height * 4);
}

/// <summary>
/// The pixel widths art decodes at. A slot decodes at a fixed width rather than
/// its laid-out width, so one cover occupies one cache entry instead of one per
/// layout pass; the control scales the decode down to fit. Each is the largest
/// the slot renders at, so a decode is never scaled up.
/// </summary>
internal static class ImageWidths
{
    /// <summary>Album-grid cards. Cells sit near
    /// <see cref="AlbumGridColumns.TargetCellWidth"/> and grow with the window;
    /// this covers that growth.</summary>
    internal const int GridTile = 320;

    /// <summary>The album-detail hero cover.</summary>
    internal const int DetailCover = 640;

    /// <summary>A 44–48px list row's thumbnail, at up to 2x.</summary>
    internal const int Row = 96;

    /// <summary>Cover-picker and triage-sidebar tiles.</summary>
    internal const int PickerTile = 240;
}

/// <summary>
/// A cover for one row of a list, held as a bindable property. Rows are built
/// before the store exists, so the content is captured at construction and the
/// store attached later; the first read starts the load and
/// <see cref="SourceChanged"/> fires when it lands.
/// </summary>
internal sealed class ImageBinding(ImageContent? content, int pixelWidth)
{
    private readonly ImageContent? _content = content;
    private readonly int _pixelWidth = pixelWidth;
    private ImageStore? _store;
    private Dispatcher? _dispatcher;
    private Bitmap? _source;
    private bool _loading;
    private bool _loaded;

    internal event Action? SourceChanged;

    internal Bitmap? Source
    {
        get
        {
            StartLoad();
            return _source;
        }
    }

    internal void Attach(ImageStore store, Dispatcher dispatcher)
    {
        _store = store;
        _dispatcher = dispatcher;
        StartLoad();
    }

    private void StartLoad()
    {
        if (_content is null || _store is null || _dispatcher is null || _loaded || _loading)
        {
            return;
        }

        if (_store.Cached(_content, _pixelWidth) is { } cached)
        {
            _source = cached;
            _loaded = true;
            SourceChanged?.Invoke();
            return;
        }

        _loading = true;
        var store = _store;
        var dispatcher = _dispatcher;
        var content = _content;
        _ = store.LoadAsync(content, _pixelWidth).ContinueWith(
            task =>
            {
                var bitmap = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                dispatcher.Post(() =>
                {
                    _source = bitmap;
                    _loaded = true;
                    _loading = false;
                    SourceChanged?.Invoke();
                });
            },
            TaskScheduler.Default);
    }
}
