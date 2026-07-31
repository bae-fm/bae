namespace Bae.Desktop;

// Middle-truncate a value so both ends stay visible: catalog numbers and barcodes
// differ at the end, which an end-ellipsis would hide. The toolkit's TextTrimming
// has no middle mode, and the signal-badge value is monospace (Consolas), so a
// character budget tracks the pixel width closely. Mirrors macOS's
// .truncationMode(.middle) on the signal value. No Avalonia or bridge types, so
// it is unit-tested.
public static class TextTruncation
{
    public static string MiddleTruncate(string value, int maxChars)
    {
        if (value.Length <= maxChars)
        {
            return value;
        }

        var keep = maxChars - 1; // one character for the ellipsis
        var head = keep / 2;
        var tail = keep - head;
        return value[..head] + "…" + value[^tail..];
    }
}
