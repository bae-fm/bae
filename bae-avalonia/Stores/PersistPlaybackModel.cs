using System;

namespace Bae.Desktop;

// Pure token parse/serialize for the "restore on launch" playback preference.
// No WinRT, no I/O — the store layers the file read/write on top of this.
public static class PersistPlaybackModel
{
    // The persisted preference, as the token the store writes to disk and reads
    // back. Unknown or missing tokens fall back to off.
    public static string Token(bool persist) =>
        persist ? "on" : "off";

    public static bool PersistFromToken(string? token) =>
        string.Equals(token?.Trim(), "on", StringComparison.Ordinal);
}
