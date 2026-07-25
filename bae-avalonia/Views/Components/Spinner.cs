using System;
using Avalonia;
using Avalonia.Animation;
using Avalonia.Animation.Easings;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Styling;

namespace Bae.Desktop;

// A small indeterminate spinner: a three-quarter accent arc that rotates. Used
// in-button while a long action runs (creating a library). The stroke reads the
// accent theme brush so it tracks the OS appearance.
internal sealed class Spinner : UserControl
{
    public Spinner()
    {
        var arc = new Arc
        {
            StartAngle = 0,
            SweepAngle = 270,
            StrokeThickness = 2,
            StrokeLineCap = PenLineCap.Round,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            VerticalAlignment = VerticalAlignment.Stretch,
            RenderTransformOrigin = RelativePoint.Center,
        };
        arc[!Shape.StrokeProperty] = new DynamicResourceExtension("BaeAccentBrush");

        // One continuous rotation; the arc reads as "busy" even in a static frame.
        var rotate = new RotateTransform();
        arc.RenderTransform = rotate;
        var spin = new Animation
        {
            Duration = TimeSpan.FromSeconds(1),
            IterationCount = IterationCount.Infinite,
            Easing = new LinearEasing(),
            Children =
            {
                new KeyFrame { Cue = new Cue(0d), Setters = { new Setter(RotateTransform.AngleProperty, 0d) } },
                new KeyFrame { Cue = new Cue(1d), Setters = { new Setter(RotateTransform.AngleProperty, 360d) } },
            },
        };
        _ = spin.RunAsync(rotate);

        Content = arc;
    }
}
