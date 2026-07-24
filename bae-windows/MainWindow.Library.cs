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

// MainWindow: opening, switching, and tearing down the active library, the
// activation-intent handling, and the sync/banner rendering. Split out of
// MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    // Create a new library, reporting failure through the caller's surface (the
    // library manager's status line). Returns the new id, or null on failure.
    private string? CreateLibraryOrReport(Action<string> reportError) =>
        LibraryDiscovery.Create(reportError);

    // The libraries discovered on this device. Empty when discovery fails or none
    // exist; the libraries manager and the Ctrl+N switch read it.
    private List<BridgeLibrary> LoadLibraries() =>
        LibraryDiscovery.Load(message => StatusText.Text = message);

    // Open a library into this window. Reached only from a switch (SwitchLibrary,
    // after the previous library was torn down) and from the switch-to-locked
    // unlock prompt — the initial open runs in the coordinator, which builds the
    // window. A failed open lands on the status line; a locked target prompts for
    // its key (the re-open runs this same method).
    private void OpenLibrary(string libraryId)
    {
        switch (_session.OpenHandle(libraryId))
        {
            case OpenHandleResult.Failed:
                StatusText.Text = Loc.Chrome("library.open_failed");
                return;
            case OpenHandleResult.NeedsUnlock:
                // Encrypted library whose key isn't on this device: the session
                // freed the handle it made; prompt for the key rather than show a
                // half-open library. Unlocking re-opens through this same method.
                _ = _unlockDialog.Show(libraryId);
                return;
        }

        FinishOpen();
    }

    // Finish opening a library whose handle is already open: load the browse
    // content, seed playback, subscribe to core events, and report the screen.
    // Run from the constructor (the coordinator opened the handle before building
    // the window) and from a switch (OpenLibrary, after the previous library was
    // torn down). Construction order is load-bearing: the settings mirror is
    // reloaded before the now-playing bar's first tick reads its time-label mode.
    private void FinishOpen()
    {
        _libraryBrowser.LoadCurrentBrowserMode();
        _nowPlayingBar.SeedVolume();
        // Seed the config mirror: the now-playing bar reads its time-label mode
        // from it, so it must be populated before the first position tick.
        _settings.Reload();
        _nowPlayingBar.RefreshTimeLabelMode();
        _sync.Refresh();
        _session.Subscribe();
        // Host-originated telemetry: the library screen opened, through the
        // standalone sink. Infallible.
        NativeBae.ReportScreen(BaeDiagnostics.Handle, BridgeScreen.Library);
    }

    // Turn a parsed activation intent (the folder verb, a dropped-on-the-exe
    // folder, or bae://import) into the same UI action OnWindowDrop runs for a
    // dropped folder.
    internal async System.Threading.Tasks.Task HandleActivationIntent(ActivationIntent intent)
    {
        switch (intent)
        {
            case ActivationIntent.ImportFolder importFolder:
                if (CurrentHandleOrNull() == null)
                {
                    // No open library (welcome screen or an unresolved unlock):
                    // a no-op, matching the macOS handler's folder add.
                    BaeDiagnostics.Logger.Info(
                        "Ignored a folder-import activation: no library is open.");
                    return;
                }
                await _importSection.ImportFolder(importFolder.Path);
                return;
            default:
                throw new ArgumentOutOfRangeException(nameof(intent), intent, "Unknown activation intent");
        }
    }

    // Bring the window to the front for a redirected activation (a second
    // launch while bae is already running): restore it first if minimized.
    // The OS may downgrade this to a taskbar flash when foreground rights are
    // denied to a background process — accepted, not worked around.
    internal void BringToForeground()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }

        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    // Render the toolbar sync indicator and the sync banner from the sync store's
    // current state (subscribed to its Changed).
    private void RenderSyncStatus()
    {
        if (_sync.ErrorText is null)
        {
            SyncBanner.IsOpen = false;
        }
        else
        {
            var reconnect = new Button { Content = Loc.Chrome("sync.reconnect") };
            reconnect.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
            SyncBanner.Severity = InfoBarSeverity.Error;
            SyncBanner.Title = Loc.Chrome("sync.error_title");
            SyncBanner.Message = _sync.ErrorText;
            SyncBanner.ActionButton = reconnect;
            SyncBanner.IsOpen = true;
        }

        switch (_sync.Indicator)
        {
            case BridgeSyncIndicator.Error:
                SyncIndicator.Text = Loc.Chrome("sync.error_title");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                break;
            case BridgeSyncIndicator.Syncing:
                SyncIndicator.Text = Loc.Chrome("sync.syncing");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case BridgeSyncIndicator.Synced:
                SyncIndicator.Text = Loc.Chrome("sync.synced", "time", _sync.LastSyncTime);
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case BridgeSyncIndicator.Idle:
                SyncIndicator.Text = string.Empty;
                break;
        }
    }

    // Render the shell error banner from the shell store (subscribed to Changed).
    private void RenderBanner()
    {
        Banner.Severity = _shell.BannerSeverity;
        Banner.Title = _shell.BannerTitle;
        Banner.Message = _shell.BannerMessage;
        Banner.ActionButton = null;
        Banner.IsOpen = _shell.BannerIsOpen;
    }

    // Switch the active library: persist the current one's playback state, tear
    // down its handle and view state, then open the target. generated bridge init records the
    // target as the active library once it opens unlocked; a locked target lands
    // on the unlock prompt. Used for switching to an existing library and for a
    // freshly created one.
    private async System.Threading.Tasks.Task SwitchLibrary(string libraryId)
    {
        await TearDownLibrary();
        OpenLibrary(libraryId);
    }

    // Tear down the open library for a switch: shut down and free its handle, and
    // reset every piece of per-library view state so nothing from it bleeds into
    // the next library opened in this same window. (Closing the library instead
    // destroys the window, so its view state needs no reset — see the
    // coordinator's CloseLibrary.) Leaves the window with no library open.
    private async System.Threading.Tasks.Task TearDownLibrary()
    {
        await _session.ShutdownAndFreeCurrentHandle();

        _playback.Reset();
        _nowPlayingBar.Reset();
        _queuePane.Hide();
        _import.Reset();
        // The browser drops its inline expansion, browse collections, selection,
        // and search, and clears the status and shuffle-enabled surfaces.
        _libraryBrowser.Reset();
        _transferProgress.Reset();
        _mediaControls.Deactivate();
        // The banners report the old library's sync / playback errors; clear them
        // so they don't describe state the next library (or none) doesn't have.
        _shell.ClearBanner();
        _sync.Reset();
    }

    private System.Threading.Tasks.Task ShutdownAndFreeCurrentHandle() =>
        _session.ShutdownAndFreeCurrentHandle();

    private bool WithCurrentHandle(Action<AppHandle> action) =>
        _session.WithCurrentHandle(action);

    private LibraryHandle? CurrentHandleOrNull() =>
        _session.CurrentHandleOrNull();
}
