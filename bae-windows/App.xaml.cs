using System.Linq;
using Microsoft.UI.Xaml;

namespace Bae.Windows;

public partial class App : Application
{
    private MainWindow? _window;

    public App()
    {
#if DEBUG
        // In screenshot-capture mode, force the dark look (matching the macOS
        // capture) app-wide: the scene brushes resolve against the app theme, and
        // Application.RequestedTheme can only be set here, in the constructor.
        if (ShotCapture.TryGetOutputDir(Environment.GetCommandLineArgs(), out _))
        {
            RequestedTheme = ApplicationTheme.Dark;
            ShotCapture.Log("App ctor: capture mode, dark theme set");
        }
#endif
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
#if DEBUG
        ShotCapture.Log("App.OnLaunched entry");
        // Screenshot-capture mode: render the named preview scenes to PNGs and
        // exit. It never opens the keychain, a library, or ~/.bae, so it returns
        // before any of the normal library startup below runs.
        if (ShotCapture.TryGetOutputDir(Environment.GetCommandLineArgs(), out var shotsDir))
        {
            ShotCapture.Log("App.OnLaunched: capture branch taken");
            _ = ShotCapture.RunAsync(shotsDir);
            return;
        }
#endif

        // Telemetry first, from compiled-in values only, so the sink exists for
        // every later launch step (crash reporter, keyring, library open) and any
        // failure it reports.
        BaeDiagnostics.Configure();
        BaeCrashReporting.Configure();
        BaeDiagnostics.Logger.Info("application launched");

        // Register the OS credential store before any library key is read or
        // written (discovery, creation, or open), passing the sink so a
        // store-creation failure ships keyring_init_failed.
        NativeBae.Startup(BaeDiagnostics.Handle);

        // Register the bundled OAuth client credentials so coven can run the cloud
        // sign-in flows. Absent file: cloud sign-in stays unavailable.
        OAuthCreds.Register();

        // Assert the bae:// protocol and folder-verb registration on every
        // launch, gated to a real Velopack install so a dev run or loose-zip
        // copy never points the user's registry at build output. Re-writing
        // each launch keeps the command current and re-resolves the verb
        // label after a locale change.
        if (UpdateService.IsInstalled)
        {
            ProtocolRegistration.Register();
        }

        // The OS has already split this into argv for a normal launch, unlike
        // a redirected activation's raw Arguments string (which needs
        // ActivationIntentModel.SplitCommandLine first — see Program.OnActivated).
        var intent = ActivationIntentModel.Parse(
            Environment.GetCommandLineArgs().Skip(1).ToList(), Directory.Exists);

        _window = new MainWindow();
        _window.Activate();

        if (intent is not null)
        {
            _window.SetPendingLaunchIntent(intent);
        }
    }

    // Marshal a redirected activation (a second launch while this instance is
    // already running) onto the window's dispatcher: bring it forward always,
    // matching macOS, where every activation focuses the app even when the URL
    // carries no action, then dispatch the parsed intent when there is one.
    internal void HandleRedirectedActivation(ActivationIntent? intent)
    {
        var window = _window;
        if (window is null)
        {
            return;
        }

        window.DispatcherQueue.TryEnqueue(() =>
        {
            window.BringToForeground();
            if (intent is not null)
            {
                _ = window.HandleActivationIntent(intent);
            }
        });
    }
}
