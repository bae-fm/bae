using System;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;

namespace Bae.Windows;

// The pre-library welcome window: a fixed-size, non-resizable window shown at
// launch when no library opens, and again after a library closes. It hosts the
// welcome chooser (WelcomeView) plus the join / restore / unlock dialogs that
// create or open a library — the flows that used to render as an overlay inside
// MainWindow. Opening a library swaps to a freshly constructed MainWindow (the
// coordinator in App closes this window); this window is fixed-size and never
// persists its bounds.
internal sealed class WelcomeWindow
{
    // The fixed logical client size — not resizable, not maximizable. Mirrors
    // the macOS welcome window's content-sized fixed presentation.
    private const int WidthDips = 900;
    private const int HeightDips = 600;

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    private readonly Window _window;
    private readonly TextBlock _status;
    private readonly WelcomeView _welcomeView;
    private readonly UnlockDialog _unlockDialog;

    // openLibrary is the coordinator's open (App.OpenLibrary): every path here —
    // the chooser, create, restore, join, and unlock — hands it a library id and
    // lets the coordinator swap to MainWindow.
    public WelcomeWindow(Action<string> openLibrary)
    {
        _window = new Window { Title = "bae" };

        // The welcome status line: the choose/no-library prompt WelcomeView sets,
        // or a failed-open error the coordinator surfaces. Gray secondary text, as
        // the other code-built windows use.
        _status = new TextBlock
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        // The welcome content stacks vertically, centered — the status line above
        // the chooser panel WelcomeView adds into this same host.
        var host = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 12,
        };
        host.Children.Add(_status);

        var root = new Grid
        {
            // An opaque, themed page background rather than the bare window Mica,
            // since the welcome content is the whole window.
            Background = (Brush)Application.Current.Resources["ApplicationPageBackgroundThemeBrush"],
        };
        root.Children.Add(host);
        _window.Content = root;

        Func<XamlRoot?> xamlRoot = () => _window.Content.XamlRoot;
        Action<string> setStatus = text => _status.Text = text;

        var joinDialog = new JoinLibraryDialog(
            xamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(_window),
            () => _welcomeView.Dismiss(),
            openLibrary);
        var restoreCloudDialog = new RestoreFromCloudDialog(
            xamlRoot,
            () => _welcomeView.Dismiss(),
            openLibrary);
        _unlockDialog = new UnlockDialog(xamlRoot, setStatus, openLibrary);

        _welcomeView = new WelcomeView(
            host,
            setStatus,
            () => LibraryDiscovery.Load(setStatus),
            reportError => LibraryDiscovery.Create(reportError),
            openLibrary,
            () => joinDialog.Show(),
            () => restoreCloudDialog.Show());
    }

    // Show the window for the first time: activate it, pin it to the fixed size
    // (the presenter and client resize need a live XamlRoot for the scale), then
    // render the chooser.
    public void Activate()
    {
        _window.Activate();

        if (_window.AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
        }

        // AppWindow speaks physical pixels; scale the fixed logical size by the
        // live rasterization factor, as SettingsWindow does.
        var scale = _window.Content.XamlRoot?.RasterizationScale ?? 1.0;
        _window.AppWindow.ResizeClient(new SizeInt32(
            (int)(WidthDips * scale), (int)(HeightDips * scale)));

        _welcomeView.Show();
    }

    // Overwrite the status line — used to surface a failed library open when the
    // window is already up (WelcomeView.Show set the default choose/no-library
    // line first).
    public void SetStatus(string text) => _status.Text = text;

    // Prompt for the encryption key of a locked library discovered at launch (or
    // reached from the chooser). On success the unlock dialog re-opens through the
    // coordinator, which swaps to MainWindow.
    public System.Threading.Tasks.Task ShowUnlock(string libraryId) => _unlockDialog.Show(libraryId);

    // Bring the window forward for a redirected activation (a second launch while
    // bae is in the welcome flow): restore first if minimized. The OS may
    // downgrade this to a taskbar flash for a background process — accepted.
    public void BringToForeground()
    {
        if (_window.AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }

        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(_window));
    }

    public void Close() => _window.Close();
}
