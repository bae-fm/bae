using System.Globalization;
#if !BAE_PURE_FORMATTERS
using System.Resources;
using Jeffijoe.MessageFormat;
#endif

namespace Bae.Desktop;

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
/// <c>loc-gen --target resx</c>; the <c>ui.*</c> chrome keys live in the same
/// table (also generated) and bare app chrome lives in <c>Resources</c>. Each
/// table is a standard .NET <see cref="ResourceManager"/> over ResX-built
/// satellite assemblies, so <see cref="CultureInfo.CurrentUICulture"/> selects
/// the language. Each message value is an ICU MessageFormat 1 string; the
/// <see href="https://github.com/jeffijoe/messageformat.net">MessageFormat</see>
/// NuGet parses it at runtime (plural / select / named args), matching the
/// verbatim MF1 macOS/Android consume.
/// </summary>
internal static class Loc
{
#if !BAE_PURE_FORMATTERS
    // The Core table holds bridge-originated keys (core.*) and shared chrome
    // (ui.*), both generated from the master catalog; the Resources table holds
    // the bare app chrome. Both resolve through standard .NET satellite
    // assemblies keyed off CurrentUICulture — the base name is the resx's
    // manifest name (RootNamespace + the Strings/ folder + the file), the
    // invariant Core.resx / Resources.resx embedded in the main assembly and
    // each Core.<culture>.resx / Resources.<culture>.resx in its culture's
    // satellite assembly.
    private static readonly ResourceManager CoreManager =
        new("Bae.Desktop.Strings.Core", typeof(Loc).Assembly);
    private static readonly ResourceManager ChromeManager =
        new("Bae.Desktop.Strings.Resources", typeof(Loc).Assembly);

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
    internal static string Core(string key) => Lookup(CoreManager, key);

    /// <summary>
    /// The resolved string for a <c>core.*</c> / <c>ui.*</c> catalog key, with
    /// its MessageFormat arguments substituted (named args, plural/select).
    /// </summary>
    internal static string Core(string key, IReadOnlyDictionary<string, object?> args) =>
        Format(Lookup(CoreManager, key), args);

    /// <summary>One-argument convenience overload.</summary>
    internal static string Core(string key, string name, object? value) =>
        Core(key, new Dictionary<string, object?> { [name] = value });

    /// <summary>The resolved string for a bare app-chrome key in the default
    /// <c>Resources</c> table.</summary>
    internal static string Chrome(string key) => Lookup(ChromeManager, key);

    /// <summary>The resolved string for an app-chrome key in the default
    /// <c>Resources</c> table, with its MessageFormat arguments substituted.</summary>
    internal static string Chrome(string key, IReadOnlyDictionary<string, object?> args) =>
        Format(Lookup(ChromeManager, key), args);

    /// <summary>One-argument convenience overload for app chrome.</summary>
    internal static string Chrome(string key, string name, object? value) =>
        Chrome(key, new Dictionary<string, object?> { [name] = value });

    /// <summary>
    /// Resolve a key for the current UI culture, returning the key itself when it
    /// isn't found so a missing catalog entry is visible in the UI rather than
    /// silently empty. The resource name is the dotted id verbatim — .NET's
    /// <see cref="ResourceManager"/> treats it as an opaque name, unlike MRT
    /// which mapped <c>.</c> to a path separator.
    /// </summary>
    private static string Lookup(ResourceManager manager, string key)
    {
        var resolved = manager.GetString(key, CultureInfo.CurrentUICulture);
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
    /// this stays over plain BCL types so it is testable off the app runtime.
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
