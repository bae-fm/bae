using System;
using System.Collections.Generic;
using System.Text;

namespace Bae.Windows;

/// <summary>
/// A typed action decoded from a process activation — a folder verb / dropped
/// folder's command line, or a <c>bae://</c> link. Pure over plain BCL types
/// (following <see cref="UpdateFlowState"/>'s pattern), so it compiles in the
/// test project on any host; the WinRT activation plumbing that produces the
/// raw arguments and dispatches the intent lives in <c>Program</c> and
/// <c>MainWindow</c>.
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
/// </summary>
internal static class ActivationIntentModel
{
    /// <summary>
    /// Scan <paramref name="args"/> in order; the first argument that yields an
    /// intent wins. A folder-verb command line (or a folder dragged onto the
    /// exe) and a redirected raw command line both arrive as <c>argv</c>, so a
    /// single per-argument rule covers a bare folder path, the exe token that
    /// leads a redirected command line (a file, not a directory — no match),
    /// and Velopack's <c>--veloapp-*</c> flags (not a rooted path — no match).
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

    private static ActivationIntent? ParseArgument(string arg, Func<string, bool> isDirectory)
    {
        if (Uri.TryCreate(arg, UriKind.Absolute, out var uri)
            && string.Equals(uri.Scheme, "bae", StringComparison.OrdinalIgnoreCase))
        {
            return ParseBaeUri(uri, isDirectory);
        }

        return IsWindowsRootedPath(arg) && isDirectory(arg) ? new ActivationIntent.ImportFolder(arg) : null;
    }

    // The one recognized bae:// form: bae://import?path=<percent-encoded folder
    // path>. Any other host, a missing/empty path, or a path that isn't a
    // rooted existing directory is not an error — it's a focus-only activation,
    // matching macOS, where every bae:// URL is a no-op beyond bringing the app
    // forward.
    private static ActivationIntent? ParseBaeUri(Uri uri, Func<string, bool> isDirectory)
    {
        if (!string.Equals(uri.Host, "import", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        var rawPath = FirstQueryValue(uri.Query, "path");
        if (string.IsNullOrEmpty(rawPath))
        {
            return null;
        }

        var path = Uri.UnescapeDataString(rawPath);
        return IsWindowsRootedPath(path) && isDirectory(path) ? new ActivationIntent.ImportFolder(path) : null;
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

    // Windows-rooted per the model's own rule, not System.IO.Path (whose
    // rootedness semantics differ on the macOS host these tests also run on):
    // a drive letter (X:\ or X:/) or a UNC path (\\server\share...).
    private static bool IsWindowsRootedPath(string value)
    {
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

    /// <summary>
    /// Split a raw command-line string into argv tokens using Windows argv
    /// quoting rules (the same convention <c>CommandLineToArgvW</c> documents):
    /// whitespace separates tokens outside quotes; a double quote toggles
    /// quoted mode; a run of backslashes immediately before a quote collapses
    /// to half as many literal backslashes, and an odd run additionally escapes
    /// the quote as a literal character instead of toggling quoted mode (so a
    /// path ending in a backslash right before its closing quote, e.g. a UNC
    /// share root, closes the quote rather than escaping it). Needed because a
    /// redirected activation delivers <c>ILaunchActivatedEventArgs.Arguments</c>
    /// as one raw string, unlike <see cref="Environment.GetCommandLineArgs"/>,
    /// which the OS has already split for the initial launch.
    /// </summary>
    internal static IReadOnlyList<string> SplitCommandLine(string commandLine)
    {
        var tokens = new List<string>();
        if (string.IsNullOrEmpty(commandLine))
        {
            return tokens;
        }

        var current = new StringBuilder();
        var inQuotes = false;
        var hasToken = false;
        var i = 0;
        while (i < commandLine.Length)
        {
            var c = commandLine[i];

            if (c == '\\')
            {
                var backslashCount = 0;
                while (i < commandLine.Length && commandLine[i] == '\\')
                {
                    backslashCount++;
                    i++;
                }

                if (i < commandLine.Length && commandLine[i] == '"')
                {
                    current.Append('\\', backslashCount / 2);
                    if (backslashCount % 2 == 1)
                    {
                        current.Append('"');
                    }
                    else
                    {
                        inQuotes = !inQuotes;
                    }
                    i++;
                }
                else
                {
                    current.Append('\\', backslashCount);
                }

                hasToken = true;
                continue;
            }

            if (c == '"')
            {
                inQuotes = !inQuotes;
                hasToken = true;
                i++;
                continue;
            }

            if (!inQuotes && char.IsWhiteSpace(c))
            {
                if (hasToken)
                {
                    tokens.Add(current.ToString());
                    current.Clear();
                    hasToken = false;
                }
                i++;
                continue;
            }

            current.Append(c);
            hasToken = true;
            i++;
        }

        if (hasToken)
        {
            tokens.Add(current.ToString());
        }

        return tokens;
    }
}
