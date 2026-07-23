using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The pane's theme-aware brushes and the automation-name helper. Split out of
// QueuePane.cs unchanged.
internal sealed partial class QueuePane
{
    // -- Brushes -----------------------------------------------------------
    //
    // macOS's fixed dark palette maps to theme-aware Fluent brushes: accent = the
    // system accent, secondary/tertiary text = the TextFillColor steps, soft fills
    // = the accent color at low opacity, neutral washes = the foreground color at
    // low opacity. Freshly-constructed brushes because a brush instance can't be
    // shared across elements once parented.
    private static Brush Accent => (Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"];

    private static Brush Secondary => (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];

    private static SolidColorBrush AccentSoftFill() =>
        new((Color)Application.Current.Resources["SystemAccentColor"]) { Opacity = 0.22 };

    private static SolidColorBrush ForegroundWash(double opacity) =>
        new((Color)Application.Current.Resources["TextFillColorPrimary"]) { Opacity = opacity };

    // The now-playing card's fill: a faint accent wash over a subtle card surface,
    // top-leading to bottom-trailing.
    private static LinearGradientBrush CardWashBrush()
    {
        var accent = (Color)Application.Current.Resources["SystemAccentColor"];
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 1) };
        brush.GradientStops.Add(new GradientStop { Color = Color.FromArgb(10, accent.R, accent.G, accent.B), Offset = 0 });
        brush.GradientStops.Add(new GradientStop
        {
            Color = (Color)Application.Current.Resources["CardBackgroundFillColorSecondary"],
            Offset = 1,
        });
        return brush;
    }

    // The progress fill: a horizontal accent gradient, accent to a lightened accent
    // (the same pair the bar's scrubber uses).
    private static LinearGradientBrush ProgressFillBrush()
    {
        var brush = new LinearGradientBrush { StartPoint = new Point(0, 0), EndPoint = new Point(1, 0) };
        brush.GradientStops.Add(new GradientStop { Color = (Color)Application.Current.Resources["SystemAccentColor"], Offset = 0 });
        brush.GradientStops.Add(new GradientStop { Color = (Color)Application.Current.Resources["SystemAccentColorLight2"], Offset = 1 });
        return brush;
    }

    private static void AutomationName(Button button, string key)
    {
        var label = Loc.Chrome(key);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
    }
}
