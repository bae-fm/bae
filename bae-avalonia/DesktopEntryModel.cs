using System.Text;

namespace Bae.Desktop;

/// <summary>
/// The text of the freedesktop desktop entry that declares this app as the
/// handler for <c>bae://</c> links and for folders, and the escaping its
/// <c>Exec</c> value needs. Pure over plain BCL types (following
/// <see cref="ActivationIntentModel"/>'s pattern), so it compiles in the test
/// project on any host; the file write, the path it points at, and the
/// desktop-database refresh live in <see cref="ProtocolRegistration"/>.
/// </summary>
internal static class DesktopEntryModel
{
    /// <summary>
    /// The entry for an app called <paramref name="name"/> launched from
    /// <paramref name="execPath"/>. The two declarations are the same pair the
    /// Windows registration writes: the <c>bae</c> URL scheme, and folders. Both
    /// arrive as one argument the app parses, hence <c>%u</c> — a <c>bae://</c>
    /// link, or the URL of the folder being opened. No <c>Icon</c>: the app's
    /// icon file exists only inside the running AppImage's mount, which is gone
    /// by the time anything reads this entry.
    /// </summary>
    internal static string Build(string name, string execPath) =>
        "[Desktop Entry]\n"
        + "Type=Application\n"
        + "Version=1.5\n"
        + $"Name={EscapeValue(name)}\n"
        + $"Exec={QuoteExecArgument(execPath)} %u\n"
        + "Terminal=false\n"
        + "MimeType=x-scheme-handler/bae;inode/directory;\n";

    /// <summary>
    /// <paramref name="argument"/> written as one <c>Exec</c> argument, through
    /// the two escaping layers the specification stacks on that key. It is quoted
    /// whole so that a path holding any of the reserved characters (a space above
    /// all) stays a single argument; inside the quotes a double quote, backtick,
    /// dollar sign and backslash each take a leading backslash; and then the
    /// general escaping every string value gets doubles each backslash the
    /// quoting produced. That stacking is why a literal backslash in the path
    /// ends up as four and a literal dollar sign as two plus the sign. A percent
    /// sign is doubled as well, so it is read as itself rather than as the field
    /// code that expansion looks for once the quoting has been undone.
    /// </summary>
    internal static string QuoteExecArgument(string argument)
    {
        var quoted = new StringBuilder("\"");
        foreach (var character in argument)
        {
            if (character == '%')
            {
                quoted.Append('%');
            }
            else if (character is '"' or '`' or '$' or '\\')
            {
                quoted.Append('\\');
            }

            quoted.Append(character);
        }

        return EscapeValue(quoted.Append('"').ToString());
    }

    /// <summary>
    /// A value written as the specification's <c>string</c> type: the four
    /// characters it defines an escape sequence for, escaped. Backslashes go
    /// first, so the ones the other three sequences introduce are not doubled in
    /// turn.
    /// </summary>
    private static string EscapeValue(string value) => value
        .Replace("\\", "\\\\")
        .Replace("\n", "\\n")
        .Replace("\r", "\\r")
        .Replace("\t", "\\t");
}
