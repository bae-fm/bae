using System;
using System.IO;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Bae.Windows;

/// <summary>
/// Decodes a library cover into a <see cref="BitmapImage"/> for the WinUI image
/// controls, busting WinUI's image cache when the cover changes.
///
/// A cover lives at a stable, content-addressed path, so changing it overwrites
/// the file in place — the path never changes. WinUI's <c>BitmapImage</c> caches
/// decoded images process-wide keyed by their <c>UriSource</c>, so
/// <c>new BitmapImage(new Uri(path))</c> would keep serving the stale cover until
/// the app restarted.
///
/// The FFI hands us a cache-bustable identifier (<c>&lt;path&gt;#v=&lt;mtime&gt;</c>,
/// see bae-core's <c>versioned_image_path</c>): the version changes whenever the
/// bytes change, so the binding that produces the identifier re-evaluates and
/// calls back in here. We decode from a file <em>stream</em> via
/// <see cref="BitmapImage.SetSource"/> rather than a <c>UriSource</c>: a stream
/// source never consults WinUI's URI cache, so the current bytes on disk are
/// always read and re-decoded. This is the same per-control decode cost as the
/// old <c>UriSource</c> path — one decode per image — without the process-wide
/// cache that caused the staleness.
/// </summary>
public static class CoverImage
{
    /// <summary>
    /// Separates the on-disk path from its cache-busting version in the FFI's
    /// cover identifier. Mirrors <c>VERSION_SEPARATOR</c> in bae-core's
    /// <c>versioned_image_path</c> and the macOS / iOS loaders.
    /// </summary>
    private const string VersionSeparator = "#v=";

    /// <summary>
    /// Decode the cover at <paramref name="identifier"/> (the FFI's
    /// <c>&lt;path&gt;#v=&lt;mtime&gt;</c> form, or a bare path), or null when the
    /// identifier is null or the file can't be read. The <c>#v=…</c> suffix is the
    /// cache key, not part of the filename, so it's stripped before opening.
    /// </summary>
    public static BitmapImage? Load(string? identifier)
    {
        if (string.IsNullOrEmpty(identifier))
        {
            return null;
        }

        var separator = identifier.IndexOf(VersionSeparator, StringComparison.Ordinal);
        var path = separator < 0 ? identifier : identifier[..separator];

        try
        {
            // OpenRead + SetSource decodes from a stream, bypassing WinUI's
            // URI-keyed image cache — so the bytes currently on disk are read,
            // not a cached decode of an earlier version at the same path.
            using var stream = File.OpenRead(path);
            var bitmap = new BitmapImage();
            bitmap.SetSource(stream.AsRandomAccessStream());
            return bitmap;
        }
        catch (Exception)
        {
            // The FFI only returns identifiers for files it found on disk, so a
            // failure here is unexpected (file deleted between resolve and load,
            // a permissions problem, or an undecodable image). This getter is
            // evaluated by an x:Bind, so any escaping exception would crash the
            // binding — render a blank tile instead.
            return null;
        }
    }
}
