using System;
using System.Collections.Generic;

namespace Bae.Desktop;

/// <summary>
/// A typed action decoded from a process activation — a folder handed to the
/// app (a folder verb, a drop on the executable, a file manager's open-with) or
/// a <c>bae://</c> link. Pure over plain BCL types (following
/// <see cref="UpdateFlowState"/>'s pattern), so it compiles in the test project
/// on any host; the argv it is decoded from arrives from the desktop lifetime
/// at launch and from the single-instance listener for a forwarded launch, and
/// acting on the intent lives in <c>App</c> and <c>MainWindow</c>.
/// </summary>
internal abstract record ActivationIntent
{
    /// <summary>Scan and, if it opens successfully, watch <paramref name="Path"/>
    /// as an import source — the same action the window-drop target runs.</summary>
    internal sealed record ImportFolder(string Path) : ActivationIntent;
}

/// <summary>
/// Turns an activation's argument list into an <see cref="ActivationIntent"/>.
/// Filesystem access never happens inside this class — every rootedness check
/// is the model's own string rule (not <see cref="System.IO.Path"/>, whose
/// rootedness semantics differ on the macOS host the unit tests run on), and
/// every directory check goes through the caller-supplied <c>isDirectory</c>
/// delegate.
///
/// The rules are platform-neutral rather than gated on the running system: the
/// app serves the same argv grammar on both desktops, so every path form either
/// of them can hand over is accepted here, and whether such a path exists is the
/// delegate's answer alone.
/// </summary>
internal static class ActivationIntentModel
{
    /// <summary>
    /// Scan <paramref name="args"/> in order; the first argument that yields an
    /// intent wins. Every activation arrives as <c>argv</c> — a folder verb, a
    /// folder dropped on the executable, a file manager's open-with, a
    /// <c>bae://</c> link, a forwarded second launch — so one per-argument rule
    /// covers all of them: a bare folder path, a <c>file://</c> or <c>bae://</c>
    /// URL, the executable token that leads a forwarded command line (a file, not
    /// a directory — no match), and Velopack's <c>--veloapp-*</c> flags (not a
    /// rooted path — no match).
    /// </summary>
    internal static ActivationIntent? Parse(IReadOnlyList<string> args, Func<string, bool> isDirectory)
    {
        foreach (var arg in args)
        {
            var intent = ParseArgument(arg, isDirectory);
            if (intent is not null)
            {
                return intent;
            }
        }

        return null;
    }

    // One gate for every argument form: whatever folder path the argument names
    // has to be one of the accepted rooted shapes and has to exist.
    private static ActivationIntent? ParseArgument(string arg, Func<string, bool> isDirectory)
    {
        var path = CandidatePath(arg);
        return path is not null && IsRootedPath(path) && isDirectory(path)
            ? new ActivationIntent.ImportFolder(path)
            : null;
    }

    // The folder path an argument names, ahead of any check that it is a shape we
    // accept or that it exists — the argument as given, the local path inside a
    // file:// URI, or the path query of bae://import.
    private static string? CandidatePath(string arg)
    {
        // A rooted path is read as a path and never as a URI. Every path form is
        // also a well-formed absolute URI (C:\Music, \\storage\share and
        // /home/me/Music all parse under the file scheme), and going through URI
        // parsing would hand the import a rewritten path: backslashes turned to
        // slashes, a share name read as a remote host, and percent sequences in a
        // folder's own name decoded away.
        if (IsRootedPath(arg) || !Uri.TryCreate(arg, UriKind.Absolute, out var uri))
        {
            return arg;
        }

        if (string.Equals(uri.Scheme, "file", StringComparison.OrdinalIgnoreCase))
        {
            return FileUriPath(uri);
        }

        return string.Equals(uri.Scheme, "bae", StringComparison.OrdinalIgnoreCase)
            ? BaeImportPath(uri)
            : null;
    }

    // The local path a file:// URI names — the form a file manager hands over for
    // a folder. An empty or localhost authority is this machine; any other names
    // another host's share, which this app has no path to scan. The path is read
    // off AbsolutePath and decoded here rather than through Uri.LocalPath, whose
    // result follows the running system's path conventions — the same reason every
    // other rule in this class is spelled out by hand.
    private static string? FileUriPath(Uri uri)
    {
        if (!string.IsNullOrEmpty(uri.Host)
            && !string.Equals(uri.Host, "localhost", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        return UnwrapDriveLetter(Uri.UnescapeDataString(uri.AbsolutePath));
    }

    // A Windows path inside a file:// URI carries a slash ahead of its drive
    // letter (/C:/Music); the path itself starts at the letter. Only a drive whose
    // colon was percent-encoded still has that slash by the time it is decoded —
    // Uri drops it for the plain form — so this runs after the decode, on both.
    private static string UnwrapDriveLetter(string path) =>
        path.Length >= 3 && path[0] == '/' && IsAsciiLetter(path[1]) && path[2] == ':'
            ? path[1..]
            : path;

    // The one recognized bae:// form: bae://import?path=<percent-encoded folder
    // path>. Any other host, or a missing/empty path, is not an error — it's a
    // focus-only activation, matching macOS, where every bae:// URL is a no-op
    // beyond bringing the app forward.
    private static string? BaeImportPath(Uri uri)
    {
        if (!string.Equals(uri.Host, "import", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        var rawPath = FirstQueryValue(uri.Query, "path");
        return string.IsNullOrEmpty(rawPath) ? null : Uri.UnescapeDataString(rawPath);
    }

    // Hand-rolled query parse (no System.Web dependency, so the model stays
    // pure BCL): the first occurrence of `name` wins, matching the contract's
    // "first path query parameter wins" — including when that first occurrence
    // is present but empty, which is why an empty value is returned as-is
    // rather than falling through to a later occurrence.
    private static string? FirstQueryValue(string query, string name)
    {
        if (string.IsNullOrEmpty(query))
        {
            return null;
        }

        foreach (var pair in query.TrimStart('?').Split('&', StringSplitOptions.RemoveEmptyEntries))
        {
            var separator = pair.IndexOf('=');
            var key = separator < 0 ? pair : pair[..separator];
            if (string.Equals(key, name, StringComparison.Ordinal))
            {
                return separator < 0 ? string.Empty : pair[(separator + 1)..];
            }
        }

        return null;
    }

    // Rooted per the model's own rule, not System.IO.Path (whose rootedness
    // semantics follow the running system, and these tests also run on macOS): a
    // POSIX absolute path (/home/me/Music), a drive letter (X:\ or X:/), or a UNC
    // path (\\server\share...). All three are accepted wherever this runs — the
    // rule is about the shape of the argument, and a shape the running system has
    // no such path for still has to clear the caller's directory check. Neither a
    // relative path nor a flag (--veloapp-updated, -v) is any of these shapes.
    private static bool IsRootedPath(string value)
    {
        if (value.Length >= 1 && value[0] == '/')
        {
            return true;
        }

        if (value.Length >= 3
            && IsAsciiLetter(value[0])
            && value[1] == ':'
            && (value[2] == '\\' || value[2] == '/'))
        {
            return true;
        }

        return value.Length >= 3 && value[0] == '\\' && value[1] == '\\' && value[2] != '\\';
    }

    private static bool IsAsciiLetter(char c) => (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
}
