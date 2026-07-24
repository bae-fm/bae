using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Microsoft.Windows.AppLifecycle;
using Velopack;
using Windows.ApplicationModel.Activation;

namespace Bae.Windows;

/// <summary>
/// The app entry point, replacing the one WinUI generates
/// (<c>DISABLE_XAML_GENERATED_MAIN</c> turns that off) so the Velopack hook runs
/// before any UI. The body otherwise replicates the generated Main: XAML process
/// checks, COM wrappers, and the dispatcher-bound <see cref="App"/> — with one
/// addition, single-instancing: a second launch redirects its activation to the
/// running instance and exits instead of starting a second window.
/// </summary>
public static class Program
{
    [DllImport("Microsoft.ui.xaml.dll")]
    private static extern void XamlCheckProcessRequirements();

    // The documented unpackaged-app redirect wait: CoWaitForMultipleObjects
    // pumps COM messages while it blocks, unlike a bare Task.Wait() on the STA
    // thread, which can deadlock RedirectActivationToAsync's own COM callback.
    [DllImport("ole32.dll")]
    private static extern int CoWaitForMultipleObjects(
        uint dwFlags, uint dwMilliseconds, uint nHandles, IntPtr[] pHandles, out uint dwIndex);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateEvent(
        IntPtr lpEventAttributes, bool bManualReset, bool bInitialState, string? lpName);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetEvent(IntPtr hEvent);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CloseHandle(IntPtr hObject);

    [STAThread]
    private static void Main(string[] args)
    {
        // Record any unhandled exception from process entry onward — before
        // Velopack, single-instancing, or the Application exists — so a startup
        // crash leaves a managed record instead of an anonymous stowed crash.
        CrashCapture.InstallProcessHandlers();

#if DEBUG
        // Screenshot-capture mode bypasses everything below — the Velopack hooks
        // and the single-instance redirect serve a real app session, not a
        // one-shot render pass, and a second running instance must not redirect
        // the capture away. Detected here, before any of that plumbing, so a log
        // line lands even if startup stalls.
        if (ShotCapture.TryGetOutputDir(args, out var captureDir))
        {
            RunCapture(captureDir);
            return;
        }
#endif

        // Must run before anything else: handles the Velopack install / update /
        // uninstall hook arguments and exits the process for them. The
        // pre-uninstall hook removes the registry state OnLaunched asserts on
        // every normal run, before any UI/resource initialization.
        VelopackApp.Build()
            .OnBeforeUninstallFastCallback(_ => ProtocolRegistration.Unregister())
            .Run();

        // AppInstance is a WinRT API; the COM wrappers it needs must be up
        // before the first call, strictly after the Velopack hook above (whose
        // own callbacks must exit the process first) and before
        // XamlCheckProcessRequirements below.
        WinRT.ComWrappersSupport.InitializeComWrappers();

        // One key per edition, so bae and baeium never redirect into each
        // other. Editions are otherwise interchangeable here: whichever one is
        // running when a bae:// link or folder verb fires is the one that
        // handles it.
        var editionKey = NativeBae.SupportsOAuthProviders() ? "bae" : "baeium";
        var activationArgs = AppInstance.GetCurrent().GetActivatedEventArgs();
        var keyInstance = AppInstance.FindOrRegisterForKey(editionKey);
        if (!keyInstance.IsCurrent)
        {
            RedirectActivationTo(activationArgs, keyInstance);
            return;
        }

        // The handler fires on a background thread; it marshals to the
        // window's DispatcherQueue itself before touching any UI.
        keyInstance.Activated += OnActivated;

        XamlCheckProcessRequirements();
        Microsoft.UI.Xaml.Application.Start(p =>
        {
            var context = new DispatcherQueueSynchronizationContext(
                DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            _ = new App();
        });
    }

#if DEBUG
    // The capture-mode startup: the minimal WinUI bring-up (COM wrappers, XAML
    // process check, Application.Start) with the Velopack hooks and single-instance
    // redirect skipped. Each stage logs to <captureDir>\capture.log so a stall is
    // attributable even though a WinExe has no visible stderr. App.OnLaunched then
    // takes the capture branch and renders the scenes.
    private static void RunCapture(string captureDir)
    {
        ShotCapture.BeginCapture(captureDir);
        ShotCapture.Log("Program.Main: capture mode; skipping Velopack + single-instancing");
        WinRT.ComWrappersSupport.InitializeComWrappers();
        ShotCapture.Log("Program.Main: ComWrappers initialized");
        XamlCheckProcessRequirements();
        ShotCapture.Log("Program.Main: XamlCheckProcessRequirements passed; starting Application");
        Microsoft.UI.Xaml.Application.Start(p =>
        {
            var context = new DispatcherQueueSynchronizationContext(
                DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            _ = new App();
        });
        ShotCapture.Log("Program.Main: Application.Start returned");
    }
#endif

    // Redirect this launch's activation to the already-running instance and
    // block until it has been received, without a bare .AsTask().Wait() on the
    // STA thread: the work runs on a pool thread and signals a Win32 event,
    // which this thread waits on through CoWaitForMultipleObjects so pending
    // COM calls (including the redirect itself) still pump while it waits.
    private static void RedirectActivationTo(AppActivationArguments args, AppInstance keyInstance)
    {
        var redirectComplete = CreateEvent(IntPtr.Zero, true, false, null);
        _ = Task.Run(() =>
        {
            keyInstance.RedirectActivationToAsync(args).AsTask().Wait();
            SetEvent(redirectComplete);
        });

        const uint CwmoDefault = 0;
        const uint Infinite = 0xFFFFFFFF;
        _ = CoWaitForMultipleObjects(CwmoDefault, Infinite, 1, new[] { redirectComplete }, out _);
        CloseHandle(redirectComplete);
    }

    // Raised on a background thread by a redirected activation from a second
    // launch. Only the Launch kind carries a command line (the only kind this
    // app registers for); anything else is ignored.
    private static void OnActivated(object? sender, AppActivationArguments args)
    {
        if (args.Kind != ExtendedActivationKind.Launch
            || args.Data is not ILaunchActivatedEventArgs launchArgs)
        {
            return;
        }

        var commandLineArgs = ActivationIntentModel.SplitCommandLine(launchArgs.Arguments);
        var intent = ActivationIntentModel.Parse(commandLineArgs, Directory.Exists);

        if (Microsoft.UI.Xaml.Application.Current is App app)
        {
            app.HandleRedirectedActivation(intent);
        }
    }
}
