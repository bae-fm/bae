using Avalonia.Controls;

namespace Bae.Desktop;

// Copy text to the OS clipboard through the control's top-level window, which is
// where the clipboard hangs off the visual tree.
internal static class ClipboardHelper
{
    internal static void CopyToClipboard(Control anchor, string text) =>
        _ = TopLevel.GetTopLevel(anchor)?.Clipboard?.SetTextAsync(text);
}
