using System;
using System.Globalization;
using System.IO;

namespace Bae.Desktop;

/// <summary>
/// What pins a cached decode to the exact bytes it came from, so no entry can
/// outlive the content behind it.
///
/// A curated library image keys on its content version (the image row's
/// <c>_updated_at</c>, which moves when the bytes change). One of a release's own
/// image files keys on its file id, which is immutable: an import mints a fresh
/// id per file and a re-import mints new ones rather than repointing an existing
/// row, so an id never comes to name different bytes. Provider art keys on its
/// URL, paired with the validator core returns alongside the bytes. A file the
/// user is previewing before import keys on its path and modification date.
///
/// Free of the generated bindings and the UI toolkit: the caller reads the
/// fields off the bridge values and hands over strings.
/// </summary>
internal static class ImageTokens
{
    internal static string Library(string imageType, string id, string version) =>
        $"library:{imageType}:{id}:{version}";

    internal static string ReleaseFile(string fileId) => $"file:{fileId}";

    internal static string Remote(string url) => $"remote:{url}";

    /// <summary>The token for the file at <paramref name="path"/>, or null when
    /// its modification date can't be read — a candidate the user moved or
    /// deleted. Nothing is cached under a date we don't have; the load path
    /// surfaces the read failure itself.</summary>
    internal static string? LocalFile(string path)
    {
        try
        {
            var info = new FileInfo(path);
            if (!info.Exists)
            {
                return null;
            }

            var modified = info.LastWriteTimeUtc.Ticks.ToString(
                CultureInfo.InvariantCulture);
            return $"path:{path}#{modified}";
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException
                or ArgumentException or NotSupportedException)
        {
            return null;
        }
    }

    /// <summary>The cache key for a decode: the content's identity plus the
    /// resolution it was decoded at, so the now-playing bar's 48px decode never
    /// serves the detail view's 400px slot, and vice versa.</summary>
    internal static string Key(string token, int pixelSize) =>
        $"{token}#{pixelSize.ToString(CultureInfo.InvariantCulture)}";
}
