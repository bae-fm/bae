using System;

namespace Bae.Desktop;

/// <summary>
/// The mapping table's columns, resolved against the width the table has to lay
/// them out in and shared by its header and every one of its rows.
///
/// The widths are the table's, not each row's: a row negotiating its own widths
/// against its own content is what puts one row's length under another row's
/// role.
///
/// One set of widths only holds at one table width, which is the whole reason
/// this is resolved rather than declared. Four columns carry the give: the
/// source and the role, and the two editable fields. At <see cref="IdealWidth"/>
/// each of them has what it asks for; every point narrower is taken off all four
/// at once, each giving up its share of the shortfall in proportion to how much
/// it has to give, so they arrive at their floors together instead of one
/// collapsing while its neighbour is still roomy. <see cref="MinimumWidth"/> is
/// where all four sit at their floor; the table is never laid out narrower than
/// that, because a column squeezed past its floor stops saying what it is there
/// to say — the pane scrolls it sideways instead.
///
/// The same four pairs and the same three rigid widths as the macOS table, so a
/// pane of a given width lays out the same on both.
/// </summary>
internal readonly record struct ImportMappingColumns(
    double Source,
    double Role,
    double Title,
    double Artist)
{
    // The position, the length, and the row's own actions. There is no
    // truncation of a track number, a running time or the two words that *are*
    // the actions that leaves something still readable, so these three are the
    // same width at every table width.
    internal const double Position = 34;
    internal const double Length = 64;
    internal const double Actions = 118;

    /// <summary>The gap between two columns.</summary>
    internal const double Spacing = 10;

    // What each column that gives asks for, and what it will come down to: a
    // file name truncated in the middle beside its audition control, a role word
    // still read as that word, and a field narrow enough to scroll under the
    // caret but wide enough to read a title in.
    private const double IdealSource = 240;
    private const double FloorSource = 110;
    private const double IdealRole = 118;
    private const double FloorRole = 60;
    private const double IdealTitle = 220;
    private const double FloorTitle = 96;
    private const double IdealArtist = 180;
    private const double FloorArtist = 72;

    // What a row spends on something that is not a column: the six gaps between
    // seven columns. The row draws its own leading edges as the host border's
    // padding, outside the grid.
    private const double Chrome = Spacing * 6;
    private const double Rigid = Position + Length + Actions;

    /// <summary>The width at which every column has what it asks for. Wider than
    /// this, the surplus is the source's and nothing else moves.</summary>
    internal const double IdealWidth =
        IdealSource + IdealRole + IdealTitle + IdealArtist + Rigid + Chrome;

    /// <summary>The width at which all four giving columns are at their floor.
    /// The table is laid out at this width even when the pane is narrower, and
    /// the pane scrolls it sideways rather than squeezing a column out of the
    /// row.</summary>
    internal const double MinimumWidth =
        FloorSource + FloorRole + FloorTitle + FloorArtist + Rigid + Chrome;

    internal static ImportMappingColumns Resolve(double tableWidth)
    {
        var width = Math.Max(tableWidth, MinimumWidth);
        var given = width < IdealWidth
            ? (IdealWidth - width)
                / ((IdealSource - FloorSource) + (IdealRole - FloorRole)
                    + (IdealTitle - FloorTitle) + (IdealArtist - FloorArtist))
            : 0;
        var role = Shrunk(IdealRole, FloorRole, given);
        var title = Shrunk(IdealTitle, FloorTitle, given);
        var artist = Shrunk(IdealArtist, FloorArtist, given);
        return new ImportMappingColumns(
            // What the other six leave. Stating the source's share as the
            // remainder rather than as its own number is what makes the seven
            // add up to the table exactly at every width: the surplus above
            // IdealWidth lands here, and nowhere else.
            Source: width - Chrome - Rigid - role - title - artist,
            Role: role,
            Title: title,
            Artist: artist);
    }

    // A column `fraction` of the way from what it asks for to what it will take.
    // Not rounded to whole points: rounding three columns and leaving the fourth
    // the remainder means widening the table by a point can *narrow* the source,
    // and a column that walks backwards as the pane is dragged wider is worse
    // than one sitting on a half-point.
    private static double Shrunk(double ideal, double floor, double fraction) =>
        ideal - (ideal - floor) * fraction;
}
