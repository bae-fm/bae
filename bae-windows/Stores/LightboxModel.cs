using System;
using System.Collections.Generic;

namespace Bae.Windows;

// The lightbox's display state: position in the entry list, zoom factor, and
// the navigation/zoom decision logic. Pure BCL (no WinRT/bridge), extracted to
// unit-test the model before the UI renders it.
public sealed record LightboxState(int Count, int Index, double Zoom);

public static class LightboxModel
{
    public const double MinZoom = 1.0;
    public const double MaxZoom = 8.0;
    public const double DoubleTapZoom = 2.5;

    // Null when count is 0; startIndex clamped into [0, count).
    public static LightboxState? Open(int count, int startIndex)
    {
        if (count <= 0)
        {
            return null;
        }

        var clampedIndex = Math.Clamp(startIndex, 0, count - 1);
        return new LightboxState(count, clampedIndex, MinZoom);
    }

    // Multiple items in the list: can navigate forward and backward.
    public static bool CanCycle(LightboxState state) => state.Count > 1;

    // Wrap-around navigation: (index ± 1 + count) % count, no-op when !CanCycle.
    // Reset zoom to MinZoom on image change.
    public static LightboxState Next(LightboxState state)
    {
        if (!CanCycle(state))
        {
            return state;
        }

        var nextIndex = (state.Index + 1) % state.Count;
        return state with { Index = nextIndex, Zoom = MinZoom };
    }

    public static LightboxState Previous(LightboxState state)
    {
        if (!CanCycle(state))
        {
            return state;
        }

        var prevIndex = (state.Index - 1 + state.Count) % state.Count;
        return state with { Index = prevIndex, Zoom = MinZoom };
    }

    // Out-of-range index is ignored; zoom resets only when the index actually changes.
    public static LightboxState Select(LightboxState state, int index)
    {
        if (index < 0 || index >= state.Count || index == state.Index)
        {
            return state;
        }

        return state with { Index = index, Zoom = MinZoom };
    }

    // Clamp the requested zoom into [MinZoom, MaxZoom].
    public static double ClampZoom(double requested) =>
        Math.Clamp(requested, MinZoom, MaxZoom);

    // While zoomed (> 1.01), double-tap resets to MinZoom; at or below that
    // threshold, double-tap zooms to DoubleTapZoom.
    public static double DoubleTapTarget(double currentZoom) =>
        currentZoom > 1.01 ? MinZoom : DoubleTapZoom;

    // Chrome (title, buttons, thumbnails) is visible only at zoom <= 1.01; above
    // that threshold it fades to opacity 0.
    public static bool ChromeVisible(double zoom) => zoom <= 1.01;

    // Counter label as (key, ICU args) so the model never touches localization
    // libraries. The key is gallery.counter; args map index → 1-based position
    // and count → total.
    public static (string Key, IReadOnlyDictionary<string, object?> Args) Counter(LightboxState state)
    {
        var args = new Dictionary<string, object?> { ["index"] = state.Index + 1, ["count"] = state.Count };
        return ("gallery.counter", args);
    }
}
