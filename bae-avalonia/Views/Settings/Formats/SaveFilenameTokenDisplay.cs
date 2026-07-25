using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Display mapping for export filename pattern tokens: the chip / add-button
// label, the sample value the preview line substitutes, and the composed
// sample filename. Mirrors macOS's BridgeSaveFilenameToken extensions.
internal static class SaveFilenameTokenDisplay
{
    // Every token, in the order the "Add:" row offers them.
    internal static readonly BridgeSaveFilenameToken[] All =
    {
        BridgeSaveFilenameToken.TrackNumber,
        BridgeSaveFilenameToken.Title,
        BridgeSaveFilenameToken.Artist,
        BridgeSaveFilenameToken.Album,
        BridgeSaveFilenameToken.Year,
        BridgeSaveFilenameToken.DiscNumber,
        BridgeSaveFilenameToken.TrackTotal,
    };

    internal static string Label(BridgeSaveFilenameToken token) => token switch
    {
        BridgeSaveFilenameToken.Title => Loc.Chrome("settings.formats.token.title"),
        BridgeSaveFilenameToken.Artist => Loc.Chrome("settings.formats.token.artist"),
        BridgeSaveFilenameToken.Album => Loc.Chrome("settings.formats.token.album"),
        BridgeSaveFilenameToken.Year => Loc.Chrome("settings.formats.token.year"),
        BridgeSaveFilenameToken.TrackNumber => Loc.Chrome("settings.formats.token.track_number"),
        BridgeSaveFilenameToken.DiscNumber => Loc.Chrome("settings.formats.token.disc_number"),
        BridgeSaveFilenameToken.TrackTotal => Loc.Chrome("settings.formats.token.track_total"),
        _ => throw new ArgumentOutOfRangeException(nameof(token), token, "Unknown filename token"),
    };

    // The sample value the preview line substitutes for a token. The numeric
    // samples stay literal — filenames aren't locale-formatted — and the track
    // number mirrors the exporter's two-digit padding.
    internal static string Sample(BridgeSaveFilenameToken token) => token switch
    {
        BridgeSaveFilenameToken.Title => Loc.Chrome("settings.formats.sample.title"),
        BridgeSaveFilenameToken.Artist => Loc.Chrome("settings.formats.sample.artist"),
        BridgeSaveFilenameToken.Album => Loc.Chrome("settings.formats.sample.album"),
        BridgeSaveFilenameToken.Year => "2020",
        BridgeSaveFilenameToken.TrackNumber => "04",
        BridgeSaveFilenameToken.DiscNumber => "1",
        BridgeSaveFilenameToken.TrackTotal => "12",
        _ => throw new ArgumentOutOfRangeException(nameof(token), token, "Unknown filename token"),
    };

    // The sample filename the preview lines show for a pattern: the tokens'
    // sample values joined with spaces (an empty pattern falls back to the
    // title, mirroring the exporter) plus the given extension.
    internal static string PreviewFilename(
        IEnumerable<BridgeSaveFilenameToken> tokens,
        string extension)
    {
        var stem = string.Join(" ", tokens.Select(Sample));
        if (stem.Length == 0)
        {
            stem = Sample(BridgeSaveFilenameToken.Title);
        }
        return $"{stem}.{extension}";
    }
}
