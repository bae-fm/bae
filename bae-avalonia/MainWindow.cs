using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The skeleton's only window: it shows what one real bridge call returns, so
/// a launch exercises the generated bindings, the native library load, and the
/// host end to end. The desktop UI itself is not built here.
/// </summary>
internal sealed class MainWindow : Window
{
    internal MainWindow(BridgeHost host)
    {
        Title = App.Edition;
        Width = 480;
        Height = 160;
        var summary = new TextBlock
        {
            Text = "Reading the libraries on this device…",
            Margin = new Thickness(24),
            TextWrapping = TextWrapping.Wrap,
        };
        Content = summary;
        // The listing runs on the host's runtime; the window shows it once the
        // read completes. An exception the summary does not handle reaches the
        // dispatcher's crash capture.
        Opened += async (_, _) => summary.Text = await Summary(host);
    }

    private static async Task<string> Summary(BridgeHost host)
    {
        try
        {
            return $"{await NativeBae.LibraryCount(host)} libraries on this device.";
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to list libraries.", exception);
            return $"Could not read the libraries on this device: {exception.Message}";
        }
    }
}
