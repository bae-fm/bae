using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;

namespace Bae.Desktop;

// The window coordinator: owns the library session and swaps between the welcome
// window (no library open) and the main window (a library open), mirroring the
// macOS two-window model. The main window is constructed only when a library
// actually opens, so launch never flashes an empty shell. The new window comes up
// before the old one closes, so the app always keeps a live window across a swap.
public sealed partial class App : Application
{
    private SessionStore? _session;
    private IMediaControl? _mediaControl;
    private UpdateService? _updateService;
    private MainWindow? _main;
    private WelcomeWindow? _welcome;

    // The intent this launch's own argv carried, held until a library opens and the
    // main window can act on it. A launch that lands on the welcome window instead
    // clears it — except a locked library, whose unlock opens the library and fires
    // the intent then.
    private ActivationIntent? _pendingLaunchIntent;

    // Set once the exit teardown has started, by whichever exit runs it — the
    // quit path or the update restart. The shutdown that follows the quit path is
    // forced and so never re-enters the handler, but a second quit request
    // arriving while the teardown is still running would, and must not run it
    // twice.
    private bool _quitting;

    private SessionStore Session => _session!;
    private IMediaControl MediaControl => _mediaControl!;
    private UpdateService Updates => _updateService!;

    // The edition key for single-instancing and per-edition state. The full
    // (OAuth) edition compiles with BAE_FULL_BRIDGE; the baeium edition doesn't.
    internal static string Edition =>
#if BAE_FULL_BRIDGE
        "bae";
#else
        "baeium";
#endif

    // Held for the process lifetime so its listener keeps accepting forwarded
    // launches; disposed implicitly on exit.
    internal static SingleInstance? SingleInstance { get; set; }

    // A redirected activation from a second launch, on the single-instance
    // listener's background thread: marshal to the UI thread and hand it to
    // whichever window is up.
    internal static void OnRedirectedActivation(ActivationIntent? intent) =>
        Dispatcher.UIThread.Post(() => (Current as App)?.HandleRedirectedActivation(intent));

    // The main window comes forward and runs the intent; the welcome window only
    // comes forward, because an import intent has no open library to act on.
    private void HandleRedirectedActivation(ActivationIntent? intent)
    {
        if (_main is { } main)
        {
            main.Activate();
            if (intent is not null)
            {
                _ = main.HandleActivationIntent(intent);
            }
            return;
        }

        _welcome?.Activate();
        if (intent is ActivationIntent.ImportFolder)
        {
            BaeDiagnostics.Logger.Info("Ignored a folder-import activation: no library is open.");
        }
    }

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        // The windowing platform is up by now, so the UI dispatcher is the real
        // one; the hook has to be in place before the first job runs through it.
        CrashCapture.InstallDispatcherHandler();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // Every quit funnels through here: the last window closing, the OS
            // asking the app to exit, and the now-playing surface's Quit command.
            desktop.ShutdownRequested += (_, args) => QuitAfterTeardown(desktop, args);
            // The now-playing surface holds an OS-visible session (a bus name on
            // Linux); releasing it on the way out is how clients learn the player
            // is gone.
            desktop.Exit += (_, _) => _mediaControl?.Dispose();
            // The same argv the single-instance election forwards when this launch
            // is the second one: a folder handed to the executable, or a bae://
            // link the OS resolved to it. The platform split it for us, unlike a
            // forwarded launch's raw command line.
            StartSession(ActivationIntentModel.Parse(
                desktop.Args ?? Array.Empty<string>(), Directory.Exists));
        }

        base.OnFrameworkInitializationCompleted();
    }

    // Tear the session down, then quit. The teardown is asynchronous and the
    // process must not exit at the first await, so the shutdown that asked is
    // cancelled, the teardown runs to completion, and only then does the app shut
    // down for real — that second shutdown is forced, so nothing can cancel it and
    // it does not come back through here.
    private async void QuitAfterTeardown(
        IClassicDesktopStyleApplicationLifetime desktop, ShutdownRequestedEventArgs args)
    {
        args.Cancel = true;
        if (_quitting)
        {
            return;
        }

        _quitting = true;
        await TearDownSession();
        desktop.Shutdown();
    }

    // Applying a staged update relaunches the app, so it is an exit like quitting
    // and runs the same teardown: ApplyAndRestart never returns, and anything the
    // teardown persists would otherwise be lost. The settings window has already
    // closed itself by the time this runs.
    private async Task ApplyUpdateAndRestart()
    {
        if (_quitting)
        {
            return;
        }

        _quitting = true;
        await TearDownSession();
        Updates.ApplyAndRestart();
    }

    // What every exit does before the process goes away.
    private async Task TearDownSession()
    {
        // Clear the now-playing surface first, so no entry for a library that is
        // going away lingers while the rest of the shutdown runs.
        MediaControl.Deactivate();
        // Flush buffered telemetry through the process-lifetime sink before the
        // library handle it was carrying events for is freed below.
        BaeDiagnostics.Flush();
        // The handle's graceful shutdown is what persists playback state for the
        // next launch. A no-op when no library is open.
        await Session.ShutdownAndFreeCurrentHandle();
        // Release the surface here rather than leaving it to the lifetime's Exit,
        // which an update restart never reaches: it hands the process to the new
        // build, which asks for the same bus name and is refused while this one
        // still holds it. Idempotent, so the Exit hook that covers an exit
        // skipping this teardown stays as it is.
        MediaControl.Dispose();
    }

    // Telemetry first (so the sink exists for every later step and any failure it
    // reports), then crash reporting, then the OS credential store before any
    // library key is read, then the OAuth client credentials. Decide the first
    // window: straight to the main window when an openable library exists, else
    // the welcome window. This launch's own intent latches until that decision
    // lands, so a folder handed to a cold start is acted on once the library it
    // imports into is open.
    private void StartSession(ActivationIntent? launchIntent)
    {
        _pendingLaunchIntent = launchIntent;

        BaeDiagnostics.Configure();
        BaeCrashReporting.Configure();
        BaeDiagnostics.Logger.Info("application launched");
        NativeBae.Startup(BaeDiagnostics.Handle);
        OAuthCreds.Register();

        // Assert this app as the OS handler for bae:// links and folders, gated to
        // a real Velopack install so a dev run or a loose copy never points the
        // user's desktop at build output. Asserting it on every launch is what keeps
        // the command naming where the app is now, and re-resolves the labels that
        // are localized after a locale change.
        if (UpdateService.IsInstalled)
        {
            ProtocolRegistration.Register();
        }

        _session = new SessionStore(Dispatcher.UIThread);
        // The OS now-playing surface is process-scoped, like the session it reads
        // through: a library switch frees and reopens the handle underneath both,
        // so the surface is built once here and stays up across the swap. Its
        // service bundles are immutable delegate sets over that session, so a
        // separate pair from the AppService's is equivalent.
        _mediaControl = MediaControlService.ForCurrentPlatform(
            Dispatcher.UIThread,
            PlaybackService.FromSession(_session),
            ImageStore.FromSession(_session),
            Edition,
            raise: () => Dispatcher.UIThread.Post(() => (_main as Window ?? _welcome)?.Activate()),
            // TryShutdown, not Shutdown: a forced shutdown skips ShutdownRequested
            // and with it the session teardown that persists playback state.
            quit: () => Dispatcher.UIThread.Post(() =>
                (ApplicationLifetime as IClassicDesktopStyleApplicationLifetime)?.TryShutdown()));

        // One update service for the process, so this launch check and the
        // settings section share a single flow. The check is silent and
        // fire-and-forget — the service catches and logs every failure, and this
        // is async I/O, so it never blocks startup — and it runs from here rather
        // than from the library window so a launch that lands on the welcome
        // window checks too.
        _updateService = new UpdateService();
        _ = _updateService.CheckInBackgroundAsync();

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

    // Open a library and land on the right window. A failed open returns to the
    // welcome window with an error; a locked library shows its unlock prompt there;
    // a successful open constructs the main window and closes the welcome window.
    private void OpenLibrary(string libraryId)
    {
        switch (Session.OpenHandle(libraryId))
        {
            case OpenHandleResult.Failed:
                GoToWelcome(errorStatus: Loc.Chrome("library.open_failed"));
                return;
            case OpenHandleResult.NeedsUnlock:
                EnsureWelcome();
                CloseMain();
                _ = _welcome!.ShowUnlock(libraryId);
                return;
        }

        var main = new MainWindow(
            Session, MediaControl, Updates, CloseLibrary, SwitchLibrary, ApplyUpdateAndRestart);
        _main = main;
        main.Show();
        // Attach from here rather than from the window's own Opened/Closed: a swap
        // opens the new window before closing the old one, so the outgoing close
        // would detach the incoming window's surface.
        MediaControl.SetWindow(main);
        CloseWelcome();

        // The window this launch was waiting for. Delivered once the swap has
        // settled rather than inline, so the import section it lands on is showing
        // over the window the user is looking at.
        if (_pendingLaunchIntent is { } launchIntent)
        {
            _pendingLaunchIntent = null;
            Dispatcher.UIThread.Post(() => _ = main.HandleActivationIntent(launchIntent));
        }
    }

    // Switch the open library for another on this device: free the current handle,
    // open the target, then close the old window. Open brings up the target window
    // (or, for a locked target, the welcome unlock prompt) before the old window
    // closes, so the app keeps a live window across the swap.
    private async Task SwitchLibrary(string libraryId)
    {
        var closing = _main;
        // The window opened below restores its placement as it is built, so this
        // window's placement has to be on disk first — its own close writes too
        // late to be seen.
        closing?.SaveWindowBoundsForSwap();
        // The library the surface is showing is going away.
        MediaControl.Deactivate();
        await Session.ShutdownAndFreeCurrentHandle();
        _main = null;
        OpenLibrary(libraryId);
        closing?.Close();
    }

    // Close the open library and return to a fresh welcome window. The welcome
    // window comes up first so the app keeps a live window across the swap.
    private async Task CloseLibrary()
    {
        var closing = _main;
        if (closing is null)
        {
            return;
        }

        MediaControl.Deactivate();
        await Session.ShutdownAndFreeCurrentHandle();
        _main = null;
        GoToWelcome(errorStatus: null);
        closing.Close();
    }

    // Land on the welcome window, which is where a launch intent runs out of road:
    // there is no library for an import to scan into, so it is logged and dropped
    // the way the same intent is when it reaches a window with no open library. The
    // unlock landing does not come through here — it keeps the intent, because its
    // unlock opens the library.
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

    private void EnsureWelcome()
    {
        if (_welcome is not null)
        {
            _welcome.Activate();
            return;
        }

        var welcome = new WelcomeWindow(OpenLibrary);
        _welcome = welcome;
        welcome.Show();
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
        // No library window means no window for the OS surface to attach to.
        MediaControl.SetWindow(null);
        closing?.Close();
    }
}
