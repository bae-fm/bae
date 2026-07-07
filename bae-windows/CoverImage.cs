using System;
using System.Collections.Generic;
using System.IO;
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
/// so <see cref="LoadByImageRef"/> keeps its own cache keyed by (id, version): a
/// changed cover is a cache miss and re-decodes, while the library grid — which
/// re-evaluates each tile's bound cover as it recycles on scroll — reuses the
/// decoded bitmap instead of re-fetching and re-decoding.
/// </summary>
internal static class CoverImage
{
    /// <summary>
    /// Decoded covers keyed by (image id, content version). The version is part of
    /// the key so a changed cover (new version) misses and re-decodes; the stale
    /// entry is never served. Only the grid tile populates this (via
    /// <see cref="LoadByImageRef"/>); the gallery slots and the version-less
    /// now-playing cover decode fresh.
    /// </summary>
    private static readonly Dictionary<(string Id, string Version), BitmapImage> Cache = new();

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
        if (Cache.TryGetValue(key, out var cached))
        {
            return cached;
        }

        var bitmap = Decode(ReadBytes(
            handle,
            appHandle => NativeBae.ImageBytes(appHandle, cover)));
        if (bitmap is not null)
        {
            Cache[key] = bitmap;
        }

        return bitmap;
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
