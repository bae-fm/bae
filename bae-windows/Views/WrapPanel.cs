using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Foundation;

namespace Bae.Windows;

// Row-wrapping panel for the filename-pattern chip field and add-token row:
// children lay out left to right at their natural size and wrap onto new rows,
// like text. WinUI ships no wrapping panel that keeps each child's own width
// (VariableSizedWrapGrid forces uniform cells), so this measures and arranges
// directly.
internal sealed class WrapPanel : Panel
{
    public double Spacing { get; set; } = 5;

    protected override Size MeasureOverride(Size availableSize)
    {
        double rowWidth = 0;
        double rowHeight = 0;
        double width = 0;
        double height = 0;
        foreach (var child in Children)
        {
            child.Measure(new Size(double.PositiveInfinity, double.PositiveInfinity));
            var size = child.DesiredSize;
            if (rowWidth > 0 && rowWidth + size.Width > availableSize.Width)
            {
                width = Math.Max(width, rowWidth - Spacing);
                height += rowHeight + Spacing;
                rowWidth = 0;
                rowHeight = 0;
            }
            rowWidth += size.Width + Spacing;
            rowHeight = Math.Max(rowHeight, size.Height);
        }
        width = Math.Max(width, rowWidth - Spacing);
        return new Size(Math.Max(0, width), height + rowHeight);
    }

    protected override Size ArrangeOverride(Size finalSize)
    {
        double x = 0;
        double y = 0;
        double rowHeight = 0;
        foreach (var child in Children)
        {
            var size = child.DesiredSize;
            if (x > 0 && x + size.Width > finalSize.Width)
            {
                x = 0;
                y += rowHeight + Spacing;
                rowHeight = 0;
            }
            child.Arrange(new Rect(x, y, size.Width, size.Height));
            x += size.Width + Spacing;
            rowHeight = Math.Max(rowHeight, size.Height);
        }
        return finalSize;
    }
}
