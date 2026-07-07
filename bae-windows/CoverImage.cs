using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Decodes a library image into a <see cref="BitmapImage"/> for the WinUI image
/// controls. The bytes come through the generated bridge: versioned library
/// images by image ref, now-playing covers by id, and gallery slots by their
/// forwarded <c>source</c>. Core reads them locality-aware, fetching and
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

    /// <summary>
    /// The decoded cover for an image reference (id + version), or null when
    /// <paramref name="cover"/> is null or the bytes can't be read or decoded.
    /// Used by the grid tile and the gallery's cover slot.
    /// </summary>
    public static BitmapImage? LoadByImageRef(LibraryHandle? handle, BridgeImageRef? cover)
    {
        if (cover is null)
        {
            return null;
        }

        var key = (cover.Id, cover.Version);
        if (TryGetCached(key, out var cached))
        {
            return cached;
        }

        var bitmap = Decode(ReadImageRefBytes(handle, cover));
        if (bitmap is not null)
        {
            CacheDecoded(key, bitmap);
        }

        return bitmap;
    }

    internal sealed class Binding
    {
        private readonly BridgeImageRef? _cover;
        private LibraryHandle? _handle;
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

        internal void Attach(LibraryHandle handle, DispatcherQueue dispatcherQueue)
        {
            _handle = handle;
            _dispatcherQueue = dispatcherQueue;
            StartLoad();
        }

        private void StartLoad()
        {
            if (_cover is null || _handle is null || _dispatcherQueue is null)
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
            var handle = _handle;
            var dispatcherQueue = _dispatcherQueue;
            var cover = _cover;
            _ = Task.Run(() => ReadImageRefBytes(handle, cover)).ContinueWith(task =>
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
    public static BitmapImage? LoadImage(LibraryHandle? handle, string? imageId)
    {
        if (string.IsNullOrEmpty(imageId))
        {
            return null;
        }

        return Decode(ReadBytes(
            handle,
            appHandle => NativeBae.CoverImageBytes(appHandle, imageId)));
    }

    /// <summary>
    /// The decoded image for a gallery slot, given the item's generated source.
    /// Decoded fresh each call; null when the bytes can't be read or decoded.
    /// </summary>
    public static BitmapImage? LoadGalleryBytes(LibraryHandle? handle, string releaseId, BridgeGallerySource source) =>
        Decode(ReadBytes(
            handle,
            appHandle => NativeBae.GalleryBytes(appHandle, releaseId, source)));

    private static byte[]? ReadImageRefBytes(LibraryHandle? handle, BridgeImageRef cover) =>
        ReadBytes(
            handle,
            appHandle => NativeBae.ImageBytes(appHandle, cover));

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

    private static byte[]? ReadBytes(LibraryHandle? handle, Func<AppHandle, byte[]?> read) =>
        handle is not null && handle.TryUse(read, out var bytes) ? bytes : null;

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
