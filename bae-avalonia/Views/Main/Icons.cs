using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The shell's fixed-meaning glyphs as vector paths (Material 24×24), so the app
// carries no icon font dependency and renders identically on every platform. The
// fill reads a theme brush, so glyphs track the OS appearance.
internal static class Icons
{
    internal const string Play = "M8 5v14l11-7z";
    internal const string Pause = "M6 19h4V5H6v14zm8-14v14h4V5h-4z";
    internal const string SkipPrevious = "M6 6h2v12H6zm3.5 6l8.5 6V6z";
    internal const string SkipNext = "M6 18l8.5-6L6 6v12zM16 6v12h2V6h-2z";
    internal const string Shuffle =
        "M10.59 9.17L5.41 4 4 5.41l5.17 5.17 1.42-1.41zM14.5 4l2.04 2.04L4 18.59 5.41 20 17.96 7.46 20 9.5V4h-5.5zm.66 6.83l-1.41 1.41 3.13 3.13L14.5 20H20v-5.5l-2.04 2.04-3.13-3.13z";
    internal const string Repeat =
        "M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z";
    // Repeat with a 1 struck through the loop: the single-track repeat mode.
    internal const string RepeatOne =
        "M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zm-4-2V9h-1l-2 1v1h1.5v4H13z";
    internal const string Search =
        "M15.5 14h-.79l-.28-.27C15.41 12.59 16 11.11 16 9.5 16 5.91 13.09 3 9.5 3S3 5.91 3 9.5 5.91 16 9.5 16c1.61 0 3.09-.59 4.23-1.57l.27.28v.79l5 4.99L20.49 19l-4.99-5zm-6 0C7.01 14 5 11.99 5 9.5S7.01 5 9.5 5 14 7.01 14 9.5 11.99 14 9.5 14z";
    internal const string Queue =
        "M15 6H3v2h12V6zm0 4H3v2h12v-2zM3 16h8v-2H3v2zM17 6v8.18c-.31-.11-.65-.18-1-.18-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3V8h3V6h-5z";
    internal const string VolumeUp =
        "M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z";
    // One wave, for a level at or below the midpoint.
    internal const string VolumeDown =
        "M18.5 12c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM5 9v6h4l5 5V4L9 9H5z";
    // The speaker struck through, for muted or silent output.
    internal const string VolumeOff =
        "M16.5 12c0-1.77-1.02-3.29-2.5-4.03v2.21l2.45 2.45c.03-.2.05-.41.05-.63zm2.5 0c0 .94-.2 1.82-.54 2.64l1.51 1.51C20.63 14.91 21 13.5 21 12c0-4.28-2.99-7.86-7-8.77v2.06c2.89.86 5 3.54 5 6.71zM4.27 3L3 4.27 7.73 9H3v6h4l5 5v-6.73l4.25 4.25c-.67.52-1.42.93-2.25 1.18v2.06c1.38-.31 2.63-.95 3.69-1.81L19.73 21 21 19.73l-9-9L4.27 3zM12 4L9.91 6.09 12 8.18V4z";
    internal const string Cast =
        "M21 3H3c-1.1 0-2 .9-2 2v3h2V5h18v14h-7v2h7c1.1 0 2-.9 2-2V5c0-1.1-.9-2-2-2zM1 18v3h3c0-1.66-1.34-3-3-3zm0-4v2c2.76 0 5 2.24 5 5h2c0-3.87-3.13-7-7-7zm0-4v2c4.97 0 9 4.03 9 9h2c0-6.08-4.93-11-11-11z";
    internal const string ChevronDown = "M7 10l5 5 5-5z";
    internal const string ChevronRight = "M10 17l5-5-5-5v10z";
    internal const string ArrowUp = "M7 14l5-5 5 5z";
    internal const string Clock =
        "M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z";
    internal const string Warning =
        "M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z";
    // A folder served over the network, for a watched root that lives on one.
    internal const string NetworkFolder =
        "M15 9H9v2H3v10h18V11h-6V9zM5 19v-6h4v6H5zm14 0h-4v-6h4v6zm-8-8V7h2V3h-2V1h4v4h-2v2h2v4h-4z";
    internal const string Folder =
        "M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z";

    // A glyph at `size` filled with a theme brush key.
    internal static PathIcon Glyph(string data, double size, string brushKey)
    {
        var icon = new PathIcon
        {
            Data = Geometry.Parse(data),
            Width = size,
            Height = size,
        };
        icon[!PathIcon.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        return icon;
    }

    // An icon-only chrome button: transparent, no border, a themed glyph inside.
    internal static Button IconButton(string data, double glyphSize, string brushKey, double box)
    {
        return new Button
        {
            Width = box,
            Height = box,
            Padding = new Thickness(0),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Center,
            VerticalContentAlignment = VerticalAlignment.Center,
            Content = Glyph(data, glyphSize, brushKey),
        };
    }
}
