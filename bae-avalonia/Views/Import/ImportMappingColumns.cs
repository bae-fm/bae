using System;

namespace Bae.Desktop;

/// <summary>
/// The widths the mapping table resolves for one pane.
///
/// Two sections share one resolution so their headers and rows line up down the
/// pane: a track row's Source cell and a file row's Role cell start at the same
/// x, which is what makes the two lists read as one table of the folder rather
/// than two unrelated grids.
///
/// The number and the length are fixed — a track number and a duration have a
/// known size and squeezing them says nothing. Title, Artist and Source give up
/// width in proportion as the pane narrows, each down to a floor below which it
/// stops being a column and starts being an ellipsis.
/// </summary>
internal readonly record struct ImportMappingColumns(
    double Title,
    double Artist,
    double Source)
{
    internal const double Position = 34;
    internal const double Length = 64;

    internal const double Spacing = 10;

    private const double IdealTitle = 220;
    private const double FloorTitle = 96;
    private const double IdealArtist = 180;
    private const double FloorArtist = 72;
    private const double IdealSource = 260;
    private const double FloorSource = 140;

    private const double Chrome = Spacing * 4;
    private const double Rigid = Position + Length;

    internal const double IdealWidth =
        IdealTitle + IdealArtist + IdealSource + Rigid + Chrome;

    internal const double MinimumWidth =
        FloorTitle + FloorArtist + FloorSource + Rigid + Chrome;

    /// <summary>What a name spans where a row has one instead of a number, a
    /// title and an artist — a file, a collapsed directory, a sheet heading its
    /// slices. The three Tracks columns and the gaps between them, so a name
    /// starts where a track's number does.</summary>
    internal double Name => Position + Title + Artist + (Spacing * 2);

    internal static ImportMappingColumns Resolve(double tableWidth)
    {
        var width = Math.Max(tableWidth, MinimumWidth);
        var given = width < IdealWidth
            ? (IdealWidth - width)
                / ((IdealTitle - FloorTitle) + (IdealArtist - FloorArtist)
                    + (IdealSource - FloorSource))
            : 0;
        var title = Shrunk(IdealTitle, FloorTitle, given);
        var artist = Shrunk(IdealArtist, FloorArtist, given);
        return new ImportMappingColumns(
            Title: title,
            Artist: artist,
            // What the others leave. Stating the last share as the remainder
            // rather than as its own number is what keeps the five columns
            // summing to the table's width at every size.
            Source: Math.Max(FloorSource, width - Chrome - Rigid - title - artist));
    }

    private static double Shrunk(double ideal, double floor, double given) =>
        Math.Max(floor, ideal - ((ideal - floor) * given));
}
