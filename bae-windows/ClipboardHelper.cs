using Windows.ApplicationModel.DataTransfer;

namespace Bae.Windows;

internal static class ClipboardHelper
{
    // Copy text to the system clipboard (a shared code the user hands to another
    // device).
    internal static void CopyToClipboard(string text)
    {
        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
    }
}
