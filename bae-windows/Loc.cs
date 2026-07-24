using System.Globalization;
#if !BAE_PURE_FORMATTERS
using Jeffijoe.MessageFormat;
using Microsoft.Windows.ApplicationModel.Resources;
#endif

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
#if !BAE_PURE_FORMATTERS
    // The Core table holds bridge-originated keys (core.*) and shared chrome
    // (ui.*); both are generated from the master catalog. Bare app chrome lives
    // in the default Resources table. The second ResourceLoader argument is the
    // .resw subtree ("Core"); the default table takes no subtree.
    //
    // The index is the SDK-built module PRI next to the exe, named after it
    // ($(AssemblyName).pri — bae-windows.pri in dev, bae.pri / baeium.pri
    // installed). It carries the resw tables together with the framework
    // resource references self-contained installs resolve WinUI theme
    // resources through; a separate strings-only resources.pri is not built —
    // it would shadow the module PRI and break those theme lookups. Passed as
    // an absolute path: a bare name resolves against the process working
    // directory, which is not the install directory when launched from
    // elsewhere (the capture harness runs the exe from the repo root).
    private static readonly string ResourceIndexPath =
        Path.ChangeExtension(Environment.ProcessPath!, ".pri");
    private static readonly ResourceLoader CoreLoader = new(ResourceIndexPath, "Core");
    private static readonly ResourceLoader ChromeLoader = new(ResourceIndexPath);

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
#endif

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
    /// The digits of a clock label, from the fields core's <c>BridgeDurationClock</c>
    /// carries: <c>:</c> between them, every field after the first padded to two
    /// digits, a leading <c>-</c> for a countdown ("3:07", "1:12:34", "-0:42").
    /// Which fields there are — whether an hours field appears at all — is core's
    /// decision, not this method's. <see cref="BridgeDisplay"/> passes them in;
    /// this stays over plain BCL types so it is testable off Windows.
    /// </summary>
    internal static string Clock(bool negative, ulong? hours, uint minutes, uint seconds)
    {
        var culture = CultureInfo.CurrentCulture;
        var sign = negative ? "-" : string.Empty;
        return hours is { } h
            ? string.Format(culture, "{0}{1}:{2:00}:{3:00}", sign, h, minutes, seconds)
            : string.Format(culture, "{0}{1}:{2:00}", sign, minutes, seconds);
    }

    /// <summary>
    /// A raw whole number formatted for the current locale (grouping separators),
    /// e.g. a sample-rate or track number. macOS uses <c>.formatted()</c>; this
    /// is the .NET equivalent.
    /// </summary>
    internal static string Number(long value) =>
        value.ToString("N0", CultureInfo.CurrentCulture);
}
