using System;

namespace Bae.Windows;

// How many columns the album grid shows and how wide each cell is, for a given
// available width. Ported from the macOS column math: cells target ~200 wide with a
// 30 gutter, and the row then fills exactly — the cell width divides the available
// content width evenly across the fitted column count, so tiles flex to fill each
// row rather than leaving a ragged trailing gap (ItemsWrapGrid lays out equal
// cells, so the gutter lives inside the cell as trailing space).
internal readonly record struct AlbumGridMetrics(int Columns, double CellWidth);

internal static class AlbumGridColumns
{
    // The size a cell aims for before the fit divides the remainder across columns.
    public const double TargetCellWidth = 200;

    // The gap between cells (and, as trailing space inside each cell, what separates
    // the art of adjacent columns).
    public const double Gutter = 30;

    // The vertical gap between rows.
    public const double RowGap = 34;

    // The inset from the content column's edges to the first/last cell.
    public const double HorizontalInset = 16;

    // The column count and equal cell width for an available width. The available
    // width is the content column's width (already capped or full); the inset is
    // stripped from both sides, columns are fitted at the target size plus one
    // gutter each, and the remaining width is divided evenly so a row fills exactly.
    public static AlbumGridMetrics Compute(double availableWidth)
    {
        var effective = availableWidth - HorizontalInset * 2;
        if (effective <= 0)
        {
            return new AlbumGridMetrics(1, TargetCellWidth);
        }
        var columns = Math.Max(1, (int)Math.Floor((effective + Gutter) / (TargetCellWidth + Gutter)));
        var cellWidth = effective / columns;
        return new AlbumGridMetrics(columns, cellWidth);
    }
}
