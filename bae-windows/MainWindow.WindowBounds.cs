using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

// MainWindow: restoring and saving the window bounds, the maximize tracking,
// and the window Closed teardown. Split out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    // Place the window at its saved bounds and maximized state. Wrapped whole:
    // placement must never take down launch, so any failure logs and leaves the
    // system's default placement.
    private void RestoreWindowBounds()
    {
        try
        {
            var plan = WindowBoundsModel.PlanRestore(WindowBoundsStore.Load(), WorkAreasPrimaryFirst());
            if (plan is null)
            {
                return;
            }

            AppWindow.MoveAndResize(new RectInt32(
                plan.Bounds.X, plan.Bounds.Y, plan.Bounds.Width, plan.Bounds.Height));

            // Apply the bounds first, then maximize, so AppWindow records the sane
            // normal bounds as its restore-down target.
            if (plan.Maximized && AppWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.Maximize();
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not restore the saved window bounds.", exception);
        }
    }

    // The current work areas, with the primary display first so a saved rect that
    // overlaps no display falls back to it.
    private static List<PixelRect> WorkAreasPrimaryFirst()
    {
        var workAreas = new List<PixelRect>();
        var primary = DisplayArea.Primary;
        if (primary is not null)
        {
            workAreas.Add(ToPixelRect(primary.WorkArea));
        }

        foreach (var display in DisplayArea.FindAll())
        {
            if (primary is null || display.DisplayId.Value != primary.DisplayId.Value)
            {
                workAreas.Add(ToPixelRect(display.WorkArea));
            }
        }

        return workAreas;
    }

    private static PixelRect ToPixelRect(RectInt32 rect) =>
        new(rect.X, rect.Y, rect.Width, rect.Height);

    // Track the window's normal bounds and maximized state as it moves and
    // resizes. A field assignment per event, no I/O: OnClosed does the single
    // write. A minimized window carries no bounds worth saving, so it leaves the
    // last-seen normal bounds and maximized flag untouched.
    private void OnAppWindowChanged(AppWindow sender, AppWindowChangedEventArgs args)
    {
        if (sender.Presenter is not OverlappedPresenter presenter
            || presenter.State == OverlappedPresenterState.Minimized)
        {
            return;
        }

        _maximized = presenter.State == OverlappedPresenterState.Maximized;

        if ((args.DidPositionChange || args.DidSizeChange)
            && presenter.State == OverlappedPresenterState.Restored)
        {
            _lastNormalBounds = new PixelRect(
                sender.Position.X, sender.Position.Y, sender.Size.Width, sender.Size.Height);
        }
    }

    private async void OnClosed(object sender, WindowEventArgs args)
    {
        if (!_closeTeardownDone && CurrentHandleOrNull() != null)
        {
            // First pass: cancel this close so the async teardown below can't
            // race process exit — `async void` would otherwise let the window
            // die at the first `await`, before the shutdown's
            // persist_playback_state write lands. Run the teardown
            // deterministically, then close again; the re-entrant call below
            // takes the fast path (no live handle) and lets the close
            // proceed.
            args.Handled = true;
            await TearDownForClose();
            Close();
            return;
        }

        // Re-entry after the teardown above, a coordinator swap that already ran
        // PrepareCloseForSwap, or a close with no library ever opened: nothing
        // left to await, so let the close proceed. Bounds are only saved here if
        // no teardown pass already ran above.
        if (!_closeTeardownDone)
        {
            SaveWindowBounds();
        }
    }

    // The window's teardown, shared by the user closing the window (app quit,
    // via OnClosed) and the coordinator swapping to the welcome window (via
    // PrepareCloseForSwap): clear the transport controls, flush telemetry, shut
    // the handle down (persisting playback), and save the window bounds. Sets
    // _closeTeardownDone so a follow-up Close() from either path proceeds without
    // repeating the work.
    private async System.Threading.Tasks.Task TearDownForClose()
    {
        // Clear the transport controls first so no ghost entry lingers during
        // shutdown. Idempotent.
        _mediaControls.Deactivate();
        // Flush buffered telemetry through the standalone sink before the library
        // handle is freed by the shutdown below.
        BaeDiagnostics.Flush();
        // Always shut down gracefully; the restore-on-launch preference gates the
        // restore at the next launch (passed to InitApp), not this save — the core
        // keeps the resume row current either way.
        await ShutdownAndFreeCurrentHandle();
        SaveWindowBounds();
        _closeTeardownDone = true;
    }

    // Called by the coordinator (App) before it closes this window to return to
    // the welcome flow: run the teardown while this is still the live window, so
    // the handle is freed and bounds saved before the swap. The coordinator's
    // follow-up Close() then takes OnClosed's already-torn-down fast path.
    internal System.Threading.Tasks.Task PrepareCloseForSwap() => TearDownForClose();

    // Persist the last-seen normal bounds and maximized state, if the window
    // ever settled into a restored position, so the next launch reopens here.
    private void SaveWindowBounds()
    {
        if (_lastNormalBounds is PixelRect normalBounds)
        {
            WindowBoundsStore.Save(WindowBoundsModel.Serialize(normalBounds, _maximized));
        }
    }
}
