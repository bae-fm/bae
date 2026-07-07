using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Velopack;

namespace Bae.Windows;

/// <summary>
/// The app entry point, replacing the one WinUI generates
/// (<c>DISABLE_XAML_GENERATED_MAIN</c> turns that off) so the Velopack hook runs
/// before any UI. The body otherwise replicates the generated Main: XAML process
/// checks, COM wrappers, and the dispatcher-bound <see cref="App"/>.
/// </summary>
public static class Program
{
    [DllImport("Microsoft.ui.xaml.dll")]
    private static extern void XamlCheckProcessRequirements();

    [STAThread]
    private static void Main(string[] args)
    {
        // Must run before anything else: handles the Velopack install / update /
        // uninstall hook arguments and exits the process for them.
        VelopackApp.Build().Run();

        XamlCheckProcessRequirements();
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Microsoft.UI.Xaml.Application.Start(p =>
        {
            var context = new DispatcherQueueSynchronizationContext(
                DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(context);
            _ = new App();
        });
    }
}
