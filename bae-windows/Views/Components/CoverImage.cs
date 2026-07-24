using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Decodes a library image into a <see cref="BitmapImage"/> for the WinUI image
/// controls. The bytes come through <see cref="MediaPathsService"/>: versioned
/// library images by image ref, now-playing covers by id, and gallery slots by
/// their forwarded <c>source</c>. Core reads them locality-aware, fetching and
/// decrypting from the cloud when they aren't on disk, so the UI never resolves
/// a filesystem path itself.
///
/// Decoding goes through an in-memory stream (<see cref="BitmapImage.SetSource"/>),
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
    private static readonly Dictionary<(string Id, string Version), BitmapImage> Cache = new();
    private static readonly object CacheGate = new();

    internal sealed class Binding
    {
        private readonly BridgeImageRef? _cover;
        private MediaPathsService? _mediaPaths;
        private DispatcherQueue? _dispatcherQueue;
        private ImageSource? _source;
        private (string Id, string Version)? _loadingKey;
        private (string Id, string Version)? _loadedKey;

        internal Binding(BridgeImageRef? cover)
        {
            _cover = cover;
        }

        internal event Action? SourceChanged;

        internal ImageSource? Source
        {
            get
            {
                StartLoad();
                return _source;
            }
        }

        internal void Attach(MediaPathsService mediaPaths, DispatcherQueue dispatcherQueue)
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
            _ = Task.Run(() => mediaPaths.FetchImageBytes(cover)).ContinueWith(task =>
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

                dispatcherQueue.TryEnqueue(() =>
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
    /// The decoded cover for an image id alone — the now-playing cover, which the
    /// event carries without a version. Decoded fresh each call (no version to
    /// cache under); null when <paramref name="imageId"/> is empty or the bytes
    /// can't be read or decoded.
    /// </summary>
    public static BitmapImage? LoadImage(MediaPathsService mediaPaths, string? imageId)
    {
        if (string.IsNullOrEmpty(imageId))
        {
            return null;
        }

        return Decode(mediaPaths.FetchCoverImageBytes(imageId));
    }

    /// <summary>
    /// Decoded covers keyed by image id alone — queue entries, whose bridge shape
    /// carries a bare id with no content version. A changed cover isn't observed
    /// mid-session (queue art is stable while a track sits in the lane), so the id
    /// is a sufficient key; the cache spares the re-read/re-decode as recycled
    /// rows rebind on scroll.
    /// </summary>
    private static readonly Dictionary<string, BitmapImage> IdCache = new();

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
    public static void BindById(Image image, MediaPathsService mediaPaths, string? imageId)
    {
        // Stamp the id this control now wants (or null when absent), so a slow
        // reply for a since-recycled row can tell it is stale.
        image.Tag = imageId;
        image.Source = null;
        if (string.IsNullOrEmpty(imageId))
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

        var dispatcher = image.DispatcherQueue;
        _ = Task.Run(() => mediaPaths.FetchCoverImageBytes(imageId))
            .ContinueWith(
                task =>
                {
                    var bytes = task.Status == TaskStatus.RanToCompletion ? task.Result : null;
                    dispatcher.TryEnqueue(() =>
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
                        // The row may have been recycled to another id (or cleared)
                        // while this loaded; apply only if the control still wants
                        // this id.
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
    public static BitmapImage? LoadGalleryBytes(MediaPathsService mediaPaths, string releaseId, BridgeGallerySource source) =>
        Decode(mediaPaths.FetchGalleryBytes(releaseId, source));

    private static bool TryGetCached((string Id, string Version) key, out BitmapImage bitmap)
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

    private static void CacheDecoded((string Id, string Version) key, BitmapImage bitmap)
    {
        lock (CacheGate)
        {
            Cache[key] = bitmap;
        }
    }

    /// <summary>
    /// Decode image bytes into a <see cref="BitmapImage"/> through an in-memory
    /// stream, or null when <paramref name="bytes"/> is null or undecodable.
    /// </summary>
    private static BitmapImage? Decode(byte[]? bytes)
    {
        if (bytes is null)
        {
            return null;
        }

        try
        {
            using var stream = new MemoryStream(bytes);
            var bitmap = new BitmapImage();
            bitmap.SetSource(stream.AsRandomAccessStream());
            return bitmap;
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
