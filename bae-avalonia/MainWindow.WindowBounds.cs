using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Threading;

// Avalonia has its own PixelRect; the bounds model's is the one this file reads
// and persists, so it gets the unambiguous name here.
using BoundsRect = Bae.Desktop.PixelRect;

namespace Bae.Desktop;

// MainWindow: restoring the saved window placement at launch, tracking it while
// the window lives, and writing it back when the window closes.
internal sealed partial class MainWindow : Window
{
    // The window's last-seen normal (restored-state) bounds and whether it is
    // maximized, sampled as the window moves, resizes and changes state, and
    // written once when the window closes. Normal bounds are recorded even while
    // maximized, so restoring down after a relaunch lands on the user's chosen
    // size rather than a display-sized rect.
    private BoundsRect? _lastNormalBounds;
    private bool _maximized;

    // Set while a placement sample is queued, so a burst of notifications costs a
    // single sample.
    private bool _boundsSampleQueued;

    // Place the window at its saved bounds and maximized state. Wrapped whole:
    // placement must never take down launch, so any failure logs and leaves the
    // default placement set by the constructor.
    private void RestoreWindowBounds()
    {
        try
        {
            var plan = WindowBoundsModel.PlanRestore(WindowBoundsStore.Load(), WorkAreasPrimaryFirst());
            if (plan is null)
            {
                return;
            }

            // The saved rect is the placement; nothing centres the window over it.
            var origin = new PixelPoint(plan.Bounds.X, plan.Bounds.Y);
            WindowStartupLocation = WindowStartupLocation.Manual;
            Position = origin;

            // Paired with SampleWindowBounds: the model is physical pixels
            // throughout, Position is physical already, and Width/Height are
            // device-independent — so the size is divided here by the scaling of
            // the display the rect lands on and multiplied back there. Both sides
            // must use the same measure, or the window drifts by the scaling factor
            // every launch.
            var scaling = Screens.ScreenFromPoint(origin)?.Scaling ?? 1.0;
            Width = plan.Bounds.Width / scaling;
            Height = plan.Bounds.Height / scaling;

            // Position and size first, then maximize, so the platform records the
            // saved rect as its restore-down target rather than a display-sized one.
            if (plan.Maximized)
            {
                WindowState = WindowState.Maximized;
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not restore the saved window bounds.", exception);
        }
    }

    // The current work areas, with the primary display first so a saved rect that
    // overlaps no display falls back to it. WorkingArea is physical pixels, the
    // measure the bounds model works in.
    private List<BoundsRect> WorkAreasPrimaryFirst() =>
        Screens.All
            .OrderByDescending(screen => screen.IsPrimary)
            .Select(screen => ToBoundsRect(screen.WorkingArea))
            .ToList();

    private static BoundsRect ToBoundsRect(Avalonia.PixelRect rect) =>
        new(rect.X, rect.Y, rect.Width, rect.Height);

    // Follow the window's placement, and write it once at the end.
    //
    // Every notification queues a sample instead of reading the placement inline:
    // a maximize arrives as a resize, a move and a window-state change, and no
    // platform guarantees the state change comes first — Win32 raises the resize
    // from the same message that later reports the new state, and X11 delivers the
    // two as unordered events. A placement read inside the resize would therefore
    // record the maximized rect as the normal bounds, which is exactly the value
    // the restore-down target depends on. Reading once the notifications have
    // drained sees the settled state.
    //
    // The last sample is taken in Closing, not Closed: by Closed the platform
    // window is gone, Position reads as the origin and RenderScaling as 1. Closed
    // only writes the fields, and it fires both when the user quits and when the
    // coordinator closes this window for a library swap.
    private void TrackWindowBounds()
    {
        PositionChanged += (_, _) => QueueWindowBoundsSample();
        Resized += (_, _) => QueueWindowBoundsSample();
        PropertyChanged += (_, change) =>
        {
            if (change.Property == WindowStateProperty)
            {
                QueueWindowBoundsSample();
            }
        };
        Closing += (_, _) => SampleWindowBounds();
        Closed += (_, _) => SaveWindowBounds();
    }

    private void QueueWindowBoundsSample()
    {
        if (_boundsSampleQueued)
        {
            return;
        }

        _boundsSampleQueued = true;
        Dispatcher.UIThread.Post(SampleWindowBounds);
    }

    // Read the window's settled placement into the tracked fields. A minimized
    // window carries no bounds worth saving, so it leaves both untouched; so does a
    // window whose platform half is already gone.
    private void SampleWindowBounds()
    {
        _boundsSampleQueued = false;
        if (PlatformImpl is null || WindowState == WindowState.Minimized)
        {
            return;
        }

        _maximized = WindowState == WindowState.Maximized;

        if (WindowState == WindowState.Normal)
        {
            _lastNormalBounds = new BoundsRect(
                Position.X,
                Position.Y,
                (int)Math.Round(ClientSize.Width * RenderScaling),
                (int)Math.Round(ClientSize.Height * RenderScaling));
        }
    }

    // Write this window's placement while it is still live, for the coordinator to
    // call before it builds the window that replaces this one. The replacement
    // restores from disk as it is constructed, which is before this window's own
    // close writes anything — without this the new window would open at a stale
    // placement. The close that follows re-saves the same values.
    internal void SaveWindowBoundsForSwap()
    {
        SampleWindowBounds();
        SaveWindowBounds();
    }

    // Persist the last-seen normal bounds and maximized state, if the window ever
    // settled into a restored position, so the next launch reopens here.
    private void SaveWindowBounds()
    {
        if (_lastNormalBounds is BoundsRect normalBounds)
        {
            WindowBoundsStore.Save(WindowBoundsModel.Serialize(normalBounds, _maximized));
        }
    }
}
