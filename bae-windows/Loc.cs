using System.Globalization;
using Jeffijoe.MessageFormat;
using Microsoft.Windows.ApplicationModel.Resources;

namespace Bae.Windows;

/// <summary>
/// Localized-string resolution and locale-aware value formatting — the one
/// computation the UI is required to do because the locale never crosses the
/// bridge. bae-core and bae-bridge emit raw numbers, typed enums, and stable
/// <c>core.*</c> / <c>ui.*</c> catalog keys; this class turns a key (and
/// its MessageFormat arguments) into a string for the current locale, and
/// formats raw byte counts / durations / numbers the way macOS does with
/// <c>ByteCountFormatter</c> / <c>DateComponentsFormatter</c>.
///
/// The <c>core.*</c> keys live in the <c>Core</c> resource table generated from
/// the master catalog (<c>bae-bridge/loc/catalog.toml</c>) by
/// <c>loc-gen --target windows</c>; the <c>ui.*</c> chrome keys live in the same
/// table (also generated) and bare app chrome lives in <c>Resources</c>. Each
/// message value is an ICU MessageFormat 1 string; the
/// <see href="https://github.com/jeffijoe/messageformat.net">MessageFormat</see>
/// NuGet parses it at runtime (plural / select / named args), matching the
/// verbatim MF1 macOS/Android consume.
/// </summary>
internal static class Loc
{
    // The Core table holds bridge-originated keys (core.*) and shared chrome
    // (ui.*); both are generated from the master catalog. Bare app chrome lives
    // in the default Resources table. ResourceLoader maps a table name to its
    // compiled .resw subtree.
    private static readonly ResourceLoader CoreLoader = new("Core");
    private static readonly ResourceLoader ChromeLoader = new();

    // One formatter per culture name. The MessageFormat instance compiles and
    // caches each pattern; its locale drives plural-category selection ("one" vs
    // "other"), so it must track the UI culture, not the invariant one.
    private static readonly Dictionary<string, MessageFormatter> Formatters = new();
    private static readonly object FormattersLock = new();

    /// <summary>
    /// The resolved string for a <c>core.*</c> / <c>ui.*</c> catalog key, with
    /// no arguments. Use <see cref="Core(string, IReadOnlyDictionary{string, object?})"/>
    /// for a message that takes MessageFormat arguments.
    /// </summary>
    internal static string Core(string key) => Lookup(CoreLoader, key);

    /// <summary>
    /// The resolved string for a <c>core.*</c> / <c>ui.*</c> catalog key, with
    /// its MessageFormat arguments substituted (named args, plural/select).
    /// </summary>
    internal static string Core(string key, IReadOnlyDictionary<string, object?> args) =>
        Format(Lookup(CoreLoader, key), args);

    /// <summary>One-argument convenience overload.</summary>
    internal static string Core(string key, string name, object? value) =>
        Core(key, new Dictionary<string, object?> { [name] = value });

    /// <summary>The resolved string for a bare app-chrome key in the default
    /// <c>Resources</c> table.</summary>
    internal static string Chrome(string key) => Lookup(ChromeLoader, key);

    /// <summary>The resolved string for an app-chrome key in the default
    /// <c>Resources</c> table, with its MessageFormat arguments substituted.</summary>
    internal static string Chrome(string key, IReadOnlyDictionary<string, object?> args) =>
        Format(Lookup(ChromeLoader, key), args);

    /// <summary>One-argument convenience overload for app chrome.</summary>
    internal static string Chrome(string key, string name, object? value) =>
        Chrome(key, new Dictionary<string, object?> { [name] = value });

    /// <summary>
    /// Resolve a key, returning the key itself when it isn't found so a missing
    /// catalog entry is visible in the UI rather than silently empty. The
    /// resource name is the dotted id with <c>.</c>/<c>-</c> mapped to <c>/</c>
    /// because <c>ResourceLoader.GetString</c> treats <c>/</c> and <c>.</c> as
    /// the resource-map path separator; resw <c>name</c>s keep the dotted id, so
    /// the path form addresses the same entry.
    /// </summary>
    private static string Lookup(ResourceLoader loader, string key)
    {
        var resolved = loader.GetString(key.Replace('.', '/'));
        return string.IsNullOrEmpty(resolved) ? key : resolved;
    }

    /// <summary>
    /// Substitute MessageFormat arguments into a resolved pattern for the
    /// current UI culture. The MF1 pattern is the verbatim catalog value.
    /// </summary>
    private static string Format(string pattern, IReadOnlyDictionary<string, object?> args)
    {
        var culture = CultureInfo.CurrentUICulture.Name;
        MessageFormatter formatter;
        lock (FormattersLock)
        {
            if (!Formatters.TryGetValue(culture, out formatter!))
            {
                // useCache: the formatter memoizes each compiled pattern; locale
                // is the BCP-47 name so plural categories resolve per locale.
                formatter = new MessageFormatter(useCache: true, locale: culture);
                Formatters[culture] = formatter;
            }
        }

        // MessageFormat.NET takes IReadOnlyDictionary<string, object?>; pass the
        // args through with non-null values (a null arg renders as empty, which
        // is the right behavior for an optional substitution).
        var data = new Dictionary<string, object?>(args.Count);
        foreach (var (name, value) in args)
        {
            data[name] = value ?? string.Empty;
        }
        return formatter.FormatMessage(pattern, data);
    }

    // ── Value formatters (locale-aware) ──────────────────────────────────────

    /// <summary>
    /// A raw byte count formatted for the current locale, e.g. "412 MB" — the
    /// analog of macOS's <c>ByteCountFormatter(.file)</c>. Decimal (SI) units to
    /// match the byte counts the storage UI shows elsewhere.
    /// </summary>
    internal static string Bytes(long bytes)
    {
        var b = bytes < 0 ? 0 : bytes;
        string[] units = { "B", "KB", "MB", "GB", "TB" };
        double value = b;
        var unit = 0;
        while (value >= 1000 && unit < units.Length - 1)
        {
            value /= 1000;
            unit++;
        }
        // Whole bytes show no decimal; larger units show one. NumberFormatInfo
        // from the current culture localizes the decimal separator.
        var culture = CultureInfo.CurrentCulture;
        return unit == 0
            ? string.Format(culture, "{0:0} {1}", value, units[unit])
            : string.Format(culture, "{0:0.0} {1}", value, units[unit]);
    }

    /// <summary>
    /// A raw millisecond duration formatted as minutes:seconds for the current
    /// locale (e.g. "3:07") — the analog of macOS's
    /// <c>DateComponentsFormatter</c> in the now-playing / track-list context.
    /// Hours roll into the leading field when present. Returns an empty string
    /// for a null duration (unknown track length).
    /// </summary>
    internal static string Duration(long? milliseconds)
    {
        if (milliseconds is not { } ms)
        {
            return string.Empty;
        }
        var span = TimeSpan.FromMilliseconds(ms < 0 ? 0 : ms);
        var culture = CultureInfo.CurrentCulture;
        // "m:ss", or "h:mm:ss" once an hour accrues — the conventional
        // track/elapsed form. TimeSpan's own formatting can't drop the leading
        // zero on minutes, so compose the fields explicitly.
        return span.TotalHours >= 1
            ? string.Format(
                culture, "{0}:{1:00}:{2:00}",
                (int)span.TotalHours, span.Minutes, span.Seconds)
            : string.Format(culture, "{0}:{1:00}", span.Minutes, span.Seconds);
    }

    /// <summary>
    /// A raw whole number formatted for the current locale (grouping separators),
    /// e.g. a sample-rate or track number. macOS uses <c>.formatted()</c>; this
    /// is the .NET equivalent.
    /// </summary>
    internal static string Number(long value) =>
        value.ToString("N0", CultureInfo.CurrentCulture);
}
