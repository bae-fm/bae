using uniffi.bae_bridge;

namespace Bae.Windows;

// Display mapping for export filename pattern tokens: the chip / add-button
// label, the sample value the preview line substitutes, and the composed
// sample filename. Mirrors macOS's BridgeExportFilenameToken extensions.
internal static class ExportFilenameTokenDisplay
{
    // Every token, in the order the "Add:" row offers them.
    internal static readonly BridgeExportFilenameToken[] All =
    {
        BridgeExportFilenameToken.TrackNumber,
        BridgeExportFilenameToken.Title,
        BridgeExportFilenameToken.Artist,
        BridgeExportFilenameToken.Album,
        BridgeExportFilenameToken.Year,
        BridgeExportFilenameToken.DiscNumber,
        BridgeExportFilenameToken.TrackTotal,
    };

    internal static string Label(BridgeExportFilenameToken token) => token switch
    {
        BridgeExportFilenameToken.Title => Loc.Chrome("settings.export.token.title"),
        BridgeExportFilenameToken.Artist => Loc.Chrome("settings.export.token.artist"),
        BridgeExportFilenameToken.Album => Loc.Chrome("settings.export.token.album"),
        BridgeExportFilenameToken.Year => Loc.Chrome("settings.export.token.year"),
        BridgeExportFilenameToken.TrackNumber => Loc.Chrome("settings.export.token.track_number"),
        BridgeExportFilenameToken.DiscNumber => Loc.Chrome("settings.export.token.disc_number"),
        BridgeExportFilenameToken.TrackTotal => Loc.Chrome("settings.export.token.track_total"),
        _ => throw new ArgumentOutOfRangeException(nameof(token), token, "Unknown filename token"),
    };

    // The sample value the preview line substitutes for a token. The numeric
    // samples stay literal — filenames aren't locale-formatted — and the track
    // number mirrors the exporter's two-digit padding.
    internal static string Sample(BridgeExportFilenameToken token) => token switch
    {
        BridgeExportFilenameToken.Title => Loc.Chrome("settings.export.sample.title"),
        BridgeExportFilenameToken.Artist => Loc.Chrome("settings.export.sample.artist"),
        BridgeExportFilenameToken.Album => Loc.Chrome("settings.export.sample.album"),
        BridgeExportFilenameToken.Year => "2020",
        BridgeExportFilenameToken.TrackNumber => "04",
        BridgeExportFilenameToken.DiscNumber => "1",
        BridgeExportFilenameToken.TrackTotal => "12",
        _ => throw new ArgumentOutOfRangeException(nameof(token), token, "Unknown filename token"),
    };

    // The sample filename the preview lines show for a pattern: the tokens'
    // sample values joined with spaces (an empty pattern falls back to the
    // title, mirroring the exporter) plus the given extension.
    internal static string PreviewFilename(
        IEnumerable<BridgeExportFilenameToken> tokens,
        string extension)
    {
        var stem = string.Join(" ", tokens.Select(Sample));
        if (stem.Length == 0)
        {
            stem = Sample(BridgeExportFilenameToken.Title);
        }
        return $"{stem}.{extension}";
    }
}
