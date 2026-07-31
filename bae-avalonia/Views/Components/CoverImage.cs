using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Media.Imaging;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Decodes a library image into a <see cref="Bitmap"/> for the WinUI image
/// controls. The bytes come through <see cref="MediaPathsService"/>: versioned
/// library images by image ref, now-playing covers by id, and gallery slots by
/// their forwarded <c>source</c>. Core reads them locality-aware, fetching and
/// decrypting from the cloud when they aren't on disk, so the UI never resolves
/// a filesystem path itself.
///
/// Decoding goes through an in-memory stream (<see cref="Bitmap.SetSource"/>),
/// not a <c>UriSource</c>: WinUI caches decoded images process-wide keyed by their
/// URI, which would serve a stale cover after a change. A stream source bypasses
/// that cache. Covers carry a content version that moves when the image changes,
/// so image-ref covers keep their own cache keyed by (id, version): a changed
/// cover is a cache miss and re-decodes, while the library grid — which
/// re-evaluates each tile's bound cover as it recycles on scroll — reuses the
/// decoded bitmap instead of re-fetching and re-decoding.
/// </summary>
internal static class CoverImage
{
    /// <summary>
    /// Decoded covers keyed by (image id, content version). The version is part of
    /// the key so a changed cover (new version) misses and re-decodes; the stale
    /// entry is never served. Only image-ref covers populate this; the gallery
    /// slots and the version-less now-playing cover decode fresh.
    /// </summary>
    private static readonly Dictionary<(string Id, string Version), Bitmap> Cache = new();
    private static readonly object CacheGate = new();

    internal sealed class Binding
    {
        private readonly BridgeImageRef? _cover;
        private MediaPathsService? _mediaPaths;
        private Dispatcher? _dispatcherQueue;
        private Bitmap? _source;
        private (string Id, string Version)? _loadingKey;
        private (string Id, string Version)? _loadedKey;

        internal Binding(BridgeImageRef? cover)
        {
            _cover = cover;
        }

        internal event Action? SourceChanged;

        internal Bitmap? Source
        {
            get
            {
                StartLoad();
                return _source;
            }
        }

        internal void Attach(MediaPathsService mediaPaths, Dispatcher dispatcherQueue)
        {
            _mediaPaths = mediaPaths;
            _dispatcherQueue = dispatcherQueue;
            StartLoad();
        }

        private void StartLoad()
        {
            if (_cover is null || _mediaPaths is null || _dispatcherQueue is null)
            {
                return;
            }

            var key = (_cover.Id, _cover.Version);
            if (_loadedKey == key || _loadingKey == key)
            {
                return;
            }

            if (TryGetCached(key, out var cached))
            {
                _source = cached;
                _loadedKey = key;
                SourceChanged?.Invoke();
                return;
            }

            _loadingKey = key;
            var mediaPaths = _mediaPaths;
            var dispatcherQueue = _dispatcherQueue;
            var cover = _cover;
            _ = Task.Run(() => mediaPaths.FetchLibraryImageBytes(cover)).ContinueWith(task =>
            {
                byte[]? bytes = null;
                if (task.Status == TaskStatus.RanToCompletion)
                {
                    bytes = task.Result;
                }
                else if (task.Exception is not null)
                {
                    BaeDiagnostics.Logger.Warning("Failed to read cover image.", task.Exception);
                }

                dispatcherQueue.Post(() =>
                {
                    if (_loadingKey != key)
                    {
                        return;
                    }

                    var bitmap = Decode(bytes);
                    if (bitmap is not null)
                    {
                        CacheDecoded(key, bitmap);
                    }

                    _source = bitmap;
                    _loadedKey = key;
                    _loadingKey = null;
                    SourceChanged?.Invoke();
                });
            }, TaskScheduler.Default);
        }
    }

    /// <summary>
    /// The decoded cover for a library image ref — the now-playing cover. Decoded
    /// fresh each call; null when <paramref name="image"/> is absent or the bytes
    /// can't be read or decoded.
    /// </summary>
    public static Bitmap? LoadImage(MediaPathsService mediaPaths, BridgeImageRef? image)
    {
        if (image is null)
        {
            return null;
        }

        return Decode(mediaPaths.FetchLibraryImageBytes(image));
    }

    /// <summary>
    /// Decoded covers keyed by image ref — queue entries. The key pins the
    /// content version, so replacing a cover reloads; the cache spares the
    /// re-read/re-decode as recycled rows rebind on scroll.
    /// </summary>
    private static readonly Dictionary<string, Bitmap> IdCache = new();

    /// <summary>
    /// Bind an image control to the cover for an image id, loaded off the UI
    /// thread and applied when it lands. Recycling-safe: the requested id is
    /// stamped on the control's <see cref="Microsoft.UI.Xaml.FrameworkElement.Tag"/>
    /// at bind time, and the async reply is applied only while that stamp still
    /// matches — so if the row was recycled and rebound to another id (or cleared)
    /// before the load finished, the stale reply is dropped rather than painting
    /// the wrong cover. Clears the control first so a recycled row never shows the
    /// previous cover while the new one loads. The decoded bitmap is still cached
    /// (it is correct for its id); only the application to a since-recycled control
    /// is guarded.
    /// </summary>
    public static void BindById(Image image, MediaPathsService mediaPaths, BridgeImageRef? cover)
    {
        // Stamp the image this control now wants (or null when absent), so a slow
        // reply for a since-recycled row can tell it is stale.
        var imageId = cover is null ? null : $"{cover.Id}:{cover.Version}";
        image.Tag = imageId;
        image.Source = null;
        if (cover is null || imageId is null)
        {
            return;
        }

        lock (CacheGate)
        {
            if (IdCache.TryGetValue(imageId, out var cached))
            {
                image.Source = cached;
                return;
            }
        }

        var dispatcher = Dispatcher.UIThread;
        _ = Task.Run(() => mediaPaths.FetchLibraryImageBytes(cover))
            .ContinueWith(
                task =>
                {
                    var bytes = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                    dispatcher.Post(() =>
                    {
                        var bitmap = Decode(bytes);
                        if (bitmap is null)
                        {
                            return;
                        }
                        lock (CacheGate)
                        {
                            IdCache[imageId] = bitmap;
                        }
                        // The row may have been recycled to another image (or
                        // cleared) while this loaded; apply only if the control
                        // still wants this one.
                        if ((image.Tag as string) == imageId)
                        {
                            image.Source = bitmap;
                        }
                    });
                },
                TaskScheduler.Default);
    }

    /// <summary>
    /// The decoded image for a gallery slot, given the item's generated source.
    /// Decoded fresh each call; null when the bytes can't be read or decoded.
    /// </summary>
    public static Bitmap? LoadGalleryBytes(MediaPathsService mediaPaths, string releaseId, BridgeGallerySource source) =>
        Decode(mediaPaths.FetchReleaseImageBytes(releaseId, source));

    private static readonly System.Net.Http.HttpClient Http = new();

    /// <summary>
    /// Download and decode a remote cover thumbnail (the change-cover picker's
    /// remote candidates), whose bytes live behind an http(s) URL core built.
    /// Avalonia's <see cref="Bitmap"/> — unlike WinUI's UriSource — doesn't fetch
    /// over the network, so this reads the bytes itself. Null on a failed fetch or
    /// undecodable bytes; the caller renders a blank tile.
    /// </summary>
    public static async Task<Bitmap?> LoadUrlAsync(string url)
    {
        try
        {
            var bytes = await Http.GetByteArrayAsync(url);
            return Decode(bytes);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Failed to fetch a remote cover thumbnail.", exception);
            return null;
        }
    }

    private static bool TryGetCached((string Id, string Version) key, out Bitmap bitmap)
    {
        lock (CacheGate)
        {
            if (Cache.TryGetValue(key, out var cached))
            {
                bitmap = cached;
                return true;
            }

            bitmap = null!;
            return false;
        }
    }

    private static void CacheDecoded((string Id, string Version) key, Bitmap bitmap)
    {
        lock (CacheGate)
        {
            Cache[key] = bitmap;
        }
    }

    /// <summary>
    /// Decode image bytes into a <see cref="Bitmap"/> through an in-memory
    /// stream, or null when <paramref name="bytes"/> is null or undecodable.
    /// </summary>
    private static Bitmap? Decode(byte[]? bytes)
    {
        if (bytes is null)
        {
            return null;
        }

        try
        {
            using var stream = new MemoryStream(bytes);
            return new Bitmap(stream);
        }
        catch (Exception)
        {
            // Undecodable bytes (a corrupt or unsupported image). This getter runs
            // under an x:Bind, so an escaping exception would crash the binding —
            // render a blank tile instead.
            return null;
        }
    }
}
