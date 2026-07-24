using System.Linq;
using Microsoft.UI.Xaml;

namespace Bae.Windows;

public partial class App : Application
{
    public App()
    {
        // Record unhandled UI-dispatcher exceptions before anything else can throw
        // (the process handlers are wired earlier, in Program.Main).
        CrashCapture.InstallXamlHandler(this);
        // The fixed dark bae look is App.xaml's RequestedTheme + palette; no
        // per-mode theme forcing here.
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
#if DEBUG
        ShotCapture.Log("App.OnLaunched entry");
        // Screenshot-capture mode: render the requested preview scene(s) to PNGs
        // and exit. It never opens the keychain, a library, or ~/.bae, so it
        // returns before any of the normal library startup below runs.
        var commandLine = Environment.GetCommandLineArgs();
        if (ShotCapture.TryGetOutputDir(commandLine, out var shotsDir))
        {
            ShotCapture.Log("App.OnLaunched: capture branch taken");
            _ = ShotCapture.RunAsync(shotsDir, ShotCapture.GetSceneArg(commandLine));
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

        // Decide the first window: MainWindow when a library opens, else the
        // welcome window. MainWindow is never constructed before a library is
        // actually opening, so launch shows no empty shell. The coordinator
        // (App.Session) owns the window swap from here on.
        StartSession(intent);
    }
}
