using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The album grid's column fit: columns are chosen at the ~200 target plus a 30
// gutter each, then the cell width divides the inset-stripped width evenly so a row
// fills exactly.
public sealed class AlbumGridMetricsTests
{
    [Theory]
    // effective = width - 32; columns = floor((effective + 30) / 230).
    [InlineData(300, 1)]   // effective 268 -> floor(298/230)=1
    [InlineData(520, 2)]   // effective 488 -> floor(518/230)=2
    [InlineData(1240, 5)]  // effective 1208 -> floor(1238/230)=5
    [InlineData(1920, 8)]  // effective 1888 -> floor(1918/230)=8
    public void ColumnsFitTheTargetPlusGutter(double width, int expectedColumns)
    {
        Assert.Equal(expectedColumns, AlbumGridColumns.Compute(width).Columns);
    }

    [Fact]
    public void CellWidthFillsTheRowExactly()
    {
        var metrics = AlbumGridColumns.Compute(1240);
        var effective = 1240 - AlbumGridColumns.HorizontalInset * 2;
        Assert.Equal(effective / metrics.Columns, metrics.CellWidth, 3);
        // The columns span the whole content width — no ragged trailing gap.
        Assert.Equal(effective, metrics.CellWidth * metrics.Columns, 3);
    }

    [Fact]
    public void NeverFewerThanOneColumn()
    {
        Assert.Equal(1, AlbumGridColumns.Compute(40).Columns);
        Assert.Equal(1, AlbumGridColumns.Compute(0).Columns);
        Assert.Equal(1, AlbumGridColumns.Compute(-100).Columns);
    }
}
