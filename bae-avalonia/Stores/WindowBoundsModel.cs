using System;
using System.Collections.Generic;
using System.Text.Json;

namespace Bae.Desktop;

// A rectangle in physical (raw) pixels, the coordinate space window positions and
// display work areas are reported in. Origin can be negative (a display left of
// or above the primary).
public readonly record struct PixelRect(int X, int Y, int Width, int Height)
{
    public int Right => X + Width;

    public int Bottom => Y + Height;
}

// What the restore glue should do at launch: place the window at Bounds, and
// maximize it afterwards if Maximized. A null plan (nothing saved, or the saved
// blob failed validation) means leave the system's default placement.
public sealed record WindowRestorePlan(PixelRect Bounds, bool Maximized);

// Owns the window-bounds on-disk shape, the validation of a saved blob, and the
// clamping that keeps a restored window reachable on the current display
// arrangement. Plain BCL types only, so it is unit-tested with no windowing
// platform up; the glue that reads the real displays and places the window lives
// in MainWindow.
public static class WindowBoundsModel
{
    // The smallest saved size worth restoring: anything smaller is a degenerate
    // or garbage rect and falls back to the system's default placement.
    private const int MinWidth = 200;
    private const int MinHeight = 150;

    // A restored window must show at least this much of its width on some display
    // and keep its title bar within reach, so it can always be moved or closed.
    private const int MinVisibleWidth = 100;
    private const int TitleBarBand = 38;

    // Serialize the window's normal (restored-state) bounds and maximized flag.
    // The saved rect is always the normal bounds — never the maximized rect —
    // with the maximized state carried by the separate flag.
    public static string Serialize(PixelRect normalBounds, bool maximized)
    {
        var payload = new Dictionary<string, object>
        {
            ["x"] = normalBounds.X,
            ["y"] = normalBounds.Y,
            ["width"] = normalBounds.Width,
            ["height"] = normalBounds.Height,
            ["maximized"] = maximized,
        };
        return JsonSerializer.Serialize(payload);
    }

    // Parse a saved blob and clamp its bounds to the current display arrangement.
    // Returns null when nothing usable is saved: an absent, empty, or unparseable
    // blob; a root that is not an object; a missing/non-integer coordinate; a
    // degenerate size; or an empty work-area list. The caller passes the work
    // areas with the primary display first, so a rect intersecting none falls
    // back to it.
    public static WindowRestorePlan? PlanRestore(string? json, IReadOnlyList<PixelRect> workAreas)
    {
        if (string.IsNullOrWhiteSpace(json) || workAreas.Count == 0)
        {
            return null;
        }

        JsonDocument document;
        try
        {
            document = JsonDocument.Parse(json);
        }
        catch (JsonException)
        {
            return null;
        }

        using (document)
        {
            var root = document.RootElement;
            if (root.ValueKind != JsonValueKind.Object
                || !TryGetInt(root, "x", out var x)
                || !TryGetInt(root, "y", out var y)
                || !TryGetInt(root, "width", out var width)
                || !TryGetInt(root, "height", out var height))
            {
                return null;
            }

            if (width < MinWidth || height < MinHeight)
            {
                return null;
            }

            var maximized = root.TryGetProperty("maximized", out var flag)
                && flag.ValueKind == JsonValueKind.True;

            var saved = new PixelRect(x, y, width, height);
            return new WindowRestorePlan(Clamp(saved, workAreas), maximized);
        }
    }

    // Fit a saved rect back onto the current displays: shrink it to the target
    // work area if a monitor got smaller, then, only if too little of it (or its
    // title bar) would be reachable, move it fully inside the target. A window
    // straddling two adjacent displays stays put because it passes the reach test.
    private static PixelRect Clamp(PixelRect saved, IReadOnlyList<PixelRect> workAreas)
    {
        var target = TargetWorkArea(saved, workAreas);

        var width = Math.Min(saved.Width, target.Width);
        var height = Math.Min(saved.Height, target.Height);
        var sized = new PixelRect(saved.X, saved.Y, width, height);

        if (IsReachable(sized, workAreas))
        {
            return sized;
        }

        var clampedX = Math.Clamp(sized.X, target.X, target.Right - width);
        var clampedY = Math.Clamp(sized.Y, target.Y, target.Bottom - height);
        return new PixelRect(clampedX, clampedY, width, height);
    }

    // The work area sharing the most area with the saved rect; the first work
    // area (the primary display) when the rect overlaps none.
    private static PixelRect TargetWorkArea(PixelRect rect, IReadOnlyList<PixelRect> workAreas)
    {
        var best = workAreas[0];
        var bestArea = 0;
        foreach (var workArea in workAreas)
        {
            var area = IntersectionArea(rect, workArea);
            if (area > bestArea)
            {
                bestArea = area;
                best = workArea;
            }
        }

        return best;
    }

    // Whether some display shows enough of the rect's width and holds its top
    // edge low enough that the title bar stays grabbable.
    private static bool IsReachable(PixelRect rect, IReadOnlyList<PixelRect> workAreas)
    {
        foreach (var workArea in workAreas)
        {
            var overlap = Math.Min(rect.Right, workArea.Right) - Math.Max(rect.X, workArea.X);
            if (overlap >= MinVisibleWidth
                && rect.Y >= workArea.Y
                && rect.Y <= workArea.Bottom - TitleBarBand)
            {
                return true;
            }
        }

        return false;
    }

    private static int IntersectionArea(PixelRect a, PixelRect b)
    {
        var width = Math.Max(0, Math.Min(a.Right, b.Right) - Math.Max(a.X, b.X));
        var height = Math.Max(0, Math.Min(a.Bottom, b.Bottom) - Math.Max(a.Y, b.Y));
        return width * height;
    }

    private static bool TryGetInt(JsonElement root, string name, out int value)
    {
        value = 0;
        return root.TryGetProperty(name, out var element)
            && element.ValueKind == JsonValueKind.Number
            && element.TryGetInt32(out value);
    }
}
