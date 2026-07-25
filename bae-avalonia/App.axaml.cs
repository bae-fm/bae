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
    private MainWindow? _main;
    private WelcomeWindow? _welcome;

    private SessionStore Session => _session!;

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
    // listener's background thread: marshal to the UI thread and bring whichever
    // window is up to the front. Acting on an import / bae:// intent lands with
    // the library grid in the parity port; for now the intent is logged.
    internal static void OnRedirectedActivation(ActivationIntent? intent) =>
        Dispatcher.UIThread.Post(() => (Current as App)?.HandleRedirectedActivation(intent));

    private void HandleRedirectedActivation(ActivationIntent? intent)
    {
        if (intent is not null)
        {
            BaeDiagnostics.Logger.Info($"Redirected activation received: {intent.GetType().Name}.");
        }
        (_main as Window ?? _welcome)?.Activate();
    }

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime)
        {
            StartSession();
        }

        base.OnFrameworkInitializationCompleted();
    }

    // Telemetry first (so the sink exists for every later step and any failure it
    // reports), then the OS credential store before any library key is read, then
    // the OAuth client credentials. Decide the first window: straight to the main
    // window when an openable library exists, else the welcome window.
    private void StartSession()
    {
        BaeDiagnostics.Configure();
        BaeDiagnostics.Logger.Info("application launched");
        NativeBae.Startup(BaeDiagnostics.Handle);
        OAuthCreds.Register();

        _session = new SessionStore(Dispatcher.UIThread);

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

        var main = new MainWindow(Session, CloseLibrary);
        _main = main;
        main.Show();
        CloseWelcome();
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

        await Session.ShutdownAndFreeCurrentHandle();
        _main = null;
        GoToWelcome(errorStatus: null);
        closing.Close();
    }

    private void GoToWelcome(string? errorStatus)
    {
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
        closing?.Close();
    }
}
