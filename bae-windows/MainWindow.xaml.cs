using System;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;

namespace Bae.Windows;

/// <summary>
/// The thin host window for an open library. The coordinator (App) opens the
/// handle first and passes the already-open session in; this window builds the
/// <see cref="AppService"/> around it, hosts the <see cref="MainView"/> shell as
/// its content, and keeps only the window-level concerns: bounds restore/save,
/// presenter min-size, the <c>Closed</c> teardown, and foreground activation.
/// Constructed only when a library is actually opening, so launch never flashes
/// an empty shell.
///
/// The handle is held (by the session) for the window's lifetime and freed when
/// the window closes.
/// </summary>
public sealed partial class MainWindow : Window
{
    // The library session (handle + event subscription), owned by the
    // coordinator and reused across the window swap. The window drives its
    // teardown on close; the view drives its open-library lifecycle transitions.
    private readonly SessionStore _session;
    // Root object for the open library: the narrow domain services and the store
    // bundle, built once around the session. Held so the Closed teardown can
    // deactivate the transport controls, and handed to the view.
    private readonly AppService _appService;
    // The library shell, hosted as this window's content. Owns the element-driven
    // controllers and the open-library lifecycle transitions.
    private readonly MainView _view;

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    // Constructed only when a library is actually opening: the coordinator (App)
    // opens the handle first and passes the already-open session in, so this
    // constructor builds the AppService and view (whose construction finishes the
    // open). The launch intent is delivered after activation (enqueued below).
    // closeToWelcome returns to the welcome window via the coordinator.
    internal MainWindow(
        SessionStore session,
        Func<System.Threading.Tasks.Task> closeToWelcome,
        ActivationIntent? launchIntent)
    {
        _session = session;

        InitializeComponent();
        Closed += OnClosed;

        // The same floor the macOS main window enforces (MainWindow.minSize):
        // below it the top bar's switcher, search, and gear can't lay out.
        if (AppWindow.Presenter is OverlappedPresenter mainPresenter)
        {
            mainPresenter.PreferredMinimumWidth = 900;
            mainPresenter.PreferredMinimumHeight = 600;
        }

        // Restore the window to its last-used position, size, and maximized state
        // before the app activates it, so it appears already in place with no
        // flicker. The saved bounds are clamped to the current displays; anything
        // unusable leaves the system's default placement. Track later moves and
        // resizes so OnClosed can persist the final normal bounds.
        RestoreWindowBounds();
        AppWindow.Changed += OnAppWindowChanged;

        // The window's HWND exists at construction, so the AppService binds the
        // transport controls to it now — before the view's FinishOpen starts the
        // event stream that drives them. Both the AppService (for the transport
        // controls) and the view (for the dialogs that parent native pickers)
        // take the handle as a callback since only this window can produce it.
        Func<IntPtr> windowHandle = () => WinRT.Interop.WindowNative.GetWindowHandle(this);
        _appService = new AppService(_session, DispatcherQueue, windowHandle);
        _view = new MainView(_appService, _session, closeToWelcome, windowHandle);
        Content = _view;

        // The view's constructor loaded the browse content; finish the open now
        // that it is mounted — seed playback, subscribe to core events, report the
        // screen. (A library switch reruns both steps inside the view.)
        _view.FinishOpen();

        // Deliver a launch-time import intent after the coordinator activates the
        // window (right after this constructor returns), when the XamlRoot the
        // import dialog needs exists. Enqueued work runs once the current stack —
        // the constructor and that Activate call — unwinds.
        if (launchIntent is not null)
        {
            DispatcherQueue.TryEnqueue(() => _ = _view.HandleActivationIntent(launchIntent));
        }
    }

    // Turn a redirected activation (a second launch while bae is already running)
    // into the shell's import dispatch. The coordinator forwards it here; the view
    // owns the handling.
    internal System.Threading.Tasks.Task HandleActivationIntent(ActivationIntent intent) =>
        _view.HandleActivationIntent(intent);

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
}
