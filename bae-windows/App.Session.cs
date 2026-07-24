using System.Linq;
using Microsoft.UI.Dispatching;

namespace Bae.Windows;

// The window coordinator: owns the library session and swaps between the welcome
// window (no library open) and MainWindow (a library open), mirroring the macOS
// two-window model. MainWindow is constructed only when a library actually
// opens, so launch never flashes an empty shell.
//
// Ownership: the SessionStore (the open handle) lives here and survives the
// window swap — MainWindow receives it and reuses it. A library *switch* happens
// entirely inside MainWindow (same window); only the initial open and the close
// cross the window boundary, and those are the coordinator's. A launch-time
// import intent latches in _pendingLaunchIntent until a library opens (surviving
// a cold-start unlock), matching the old in-window latch.
public partial class App
{
    private SessionStore? _sessionStore;
    private DispatcherQueue? _dispatcher;
    private MainWindow? _main;
    private WelcomeWindow? _welcome;
    private ActivationIntent? _pendingLaunchIntent;

    private SessionStore Session => _sessionStore!;

    // Called once from OnLaunched (on the UI thread) after telemetry, the
    // keyring, and protocol registration are up. Decides the first window:
    // straight to MainWindow when an openable library exists, else the welcome
    // window. No MainWindow is constructed unless a library is actually opening.
    private void StartSession(ActivationIntent? launchIntent)
    {
        var dispatcher = DispatcherQueue.GetForCurrentThread();
        _dispatcher = dispatcher;
        _sessionStore = new SessionStore(dispatcher);
        _pendingLaunchIntent = launchIntent;

        // A library whose config.yaml won't load is listed but never auto-opened
        // (its error surfaces in the chooser), matching macOS.
        var libraries = LibraryDiscovery.Load(_ => { }).Where(library => library.Error is null).ToList();
        var openable = libraries.FirstOrDefault(library => library.IsActive)
            ?? libraries.FirstOrDefault();
        if (openable is null)
        {
            GoToWelcome(errorStatus: null);
            return;
        }

        OpenLibrary(openable.Id);
    }

    // Open a library and land on the right window. A failed open or a locked
    // library returns to the welcome window (the locked case shows its unlock
    // prompt there); a successful open constructs MainWindow and closes the
    // welcome window. Called from the launch decision, the welcome chooser (open
    // / create / restore / join), and a bootstrap unlock.
    private void OpenLibrary(string libraryId)
    {
        switch (Session.OpenHandle(libraryId))
        {
            case OpenHandleResult.Failed:
                GoToWelcome(errorStatus: Loc.Chrome("library.open_failed"));
                return;
            case OpenHandleResult.NeedsUnlock:
                GoToWelcomeForUnlock(libraryId);
                return;
        }

        // Opened: build the shell now, with the handle already open. The new
        // window comes up before the welcome window closes, so there is never a
        // moment with no window (which would exit the app). MainWindow's
        // constructor finishes the open (browse content, playback seed,
        // subscription) and delivers the launch intent once it is activated.
        var main = new MainWindow(Session, CloseLibrary, _pendingLaunchIntent);
        _pendingLaunchIntent = null;
        _main = main;
        main.Activate();
        CloseWelcome();
    }

    // Close the open library and return to a fresh welcome window. Shuts the
    // handle down (persisting playback) and saves the window bounds before the
    // main window is torn down; the welcome window comes up first so the app
    // keeps a live window across the swap. Passed to MainWindow (its close
    // button) and to the settings window's remove flow.
    private async System.Threading.Tasks.Task CloseLibrary()
    {
        var closing = _main;
        if (closing is null)
        {
            return;
        }

        await closing.PrepareCloseForSwap();
        _main = null;
        GoToWelcome(errorStatus: null);
        closing.Close();
    }

    // Ensure the welcome window is up, optionally surfacing an error, and close
    // the main window if one is still open (a failed re-open after a settings
    // lock). A launch-time import intent can't be delivered with no library open,
    // so it is dropped here with the same log the in-window handler used.
    private void GoToWelcome(string? errorStatus)
    {
        if (_pendingLaunchIntent is ActivationIntent.ImportFolder)
        {
            BaeDiagnostics.Logger.Info("Ignored a folder-import activation: no library is open.");
        }
        _pendingLaunchIntent = null;

        EnsureWelcome();
        if (errorStatus is not null)
        {
            _welcome!.SetStatus(errorStatus);
        }
        CloseMain();
    }

    // A locked library at bootstrap (or reached from the chooser): show the
    // welcome window and its unlock prompt, keeping any pending launch intent so
    // it fires once the unlock re-opens the library.
    private void GoToWelcomeForUnlock(string libraryId)
    {
        EnsureWelcome();
        CloseMain();
        _ = _welcome!.ShowUnlock(libraryId);
    }

    private void EnsureWelcome()
    {
        if (_welcome is not null)
        {
            _welcome.BringToForeground();
            return;
        }

        var welcome = new WelcomeWindow(OpenLibrary);
        _welcome = welcome;
        welcome.Activate();
    }

    private void CloseWelcome()
    {
        _welcome?.Close();
        _welcome = null;
    }

    private void CloseMain()
    {
        var closing = _main;
        _main = null;
        closing?.Close();
    }

    // A redirected activation from a second launch (bae already running),
    // marshalled from Program.OnActivated's background thread onto the UI thread.
    // It reaches whichever window is up: MainWindow brings itself forward and
    // handles an import intent; the welcome window only comes forward (an import
    // intent has no open library to act on, matching the in-window no-op).
    internal void HandleRedirectedActivation(ActivationIntent? intent)
    {
        _dispatcher?.TryEnqueue(() =>
        {
            if (_main is { } main)
            {
                main.BringToForeground();
                if (intent is not null)
                {
                    _ = main.HandleActivationIntent(intent);
                }
            }
            else if (_welcome is { } welcome)
            {
                welcome.BringToForeground();
                if (intent is ActivationIntent.ImportFolder)
                {
                    BaeDiagnostics.Logger.Info("Ignored a folder-import activation: no library is open.");
                }
            }
        });
    }
}
