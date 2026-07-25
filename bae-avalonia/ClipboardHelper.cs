using Avalonia.Controls;

namespace Bae.Desktop;

// Copy text to the OS clipboard through the control's top-level window — the
// cross-platform stand-in for the WinUI DataPackage clipboard.
internal static class ClipboardHelper
{
    internal static void CopyToClipboard(Control anchor, string text) =>
        _ = TopLevel.GetTopLevel(anchor)?.Clipboard?.SetTextAsync(text);
}
