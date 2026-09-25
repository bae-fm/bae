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
        Content = new TextBlock
        {
            Text = Summary(host),
            Margin = new Thickness(24),
            TextWrapping = TextWrapping.Wrap,
        };
    }

    private static string Summary(BridgeHost host)
    {
        try
        {
            return $"{NativeBae.LibraryCount(host)} libraries on this device.";
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to discover libraries.", exception);
            return $"Could not read the libraries on this device: {exception.Message}";
        }
    }
}
