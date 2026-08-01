using System;
using System.Collections.Generic;
using System.IO;
using System.Threading.Tasks;
using Avalonia.Controls;
using Tmds.DBus.Protocol;

namespace Bae.Desktop;

/// <summary>
/// The Linux now-playing surface: the MPRIS player object (org.mpris.MediaPlayer2
/// and its Player interface) exported on the session bus under the edition's bus
/// name. Publishes status, metadata, artwork, availability and volume as
/// properties with change signals, and raises the transport commands, seeks and
/// volume writes clients send back.
///
/// The exported state is read on the bus connection's thread and written from the
/// UI thread, so every field behind it is held under one gate. A session bus that
/// is absent, refuses the name, or fails mid-handshake leaves this session inert:
/// nothing else in the app reads it, so there is nothing half-applied to repair.
/// </summary>
internal sealed class MprisMediaSession : IMediaSession, IMethodHandler
{
    private const string MprisObjectPath = "/org/mpris/MediaPlayer2";
    private const string RootInterface = "org.mpris.MediaPlayer2";
    private const string PlayerInterface = "org.mpris.MediaPlayer2.Player";
    private const string PropertiesInterface = "org.freedesktop.DBus.Properties";

    // Each track gets its own object path so a client tells a new track from an
    // update to the one it is showing.
    private const string TrackIdPrefix = "/fm/bae/track/";

    private const string UnknownPropertyError = "org.freedesktop.DBus.Error.UnknownProperty";
    private const string ReadOnlyPropertyError = "org.freedesktop.DBus.Error.PropertyReadOnly";
    private const string InvalidArgsError = "org.freedesktop.DBus.Error.InvalidArgs";
    private const string NotSupportedError = "org.freedesktop.DBus.Error.NotSupported";

    // Ask for the name outright instead of queueing behind another owner: a name
    // already taken means something else is serving it, which is a failure to
    // report rather than a wait to sit through.
    private const uint RequestNameDoNotQueue = 0x4;
    private const uint RequestNameReplyPrimaryOwner = 1;
    private const uint RequestNameReplyAlreadyOwner = 4;

    private static readonly string[] NoStrings = Array.Empty<string>();

    private static readonly string[] RootPropertyNames =
    {
        "CanQuit", "CanRaise", "HasTrackList", "Identity", "DesktopEntry",
        "SupportedUriSchemes", "SupportedMimeTypes",
    };

    private static readonly string[] PlayerPropertyNames =
    {
        "PlaybackStatus", "Metadata", "Position", "Volume", "Rate", "MinimumRate", "MaximumRate",
        "CanGoNext", "CanGoPrevious", "CanPlay", "CanPause", "CanSeek", "CanControl",
    };

    private static readonly ReadOnlyMemory<byte> RootInterfaceXml =
        """
        <interface name="org.mpris.MediaPlayer2">
          <method name="Raise"/>
          <method name="Quit"/>
          <property name="CanQuit" type="b" access="read"/>
          <property name="CanRaise" type="b" access="read"/>
          <property name="HasTrackList" type="b" access="read"/>
          <property name="Identity" type="s" access="read"/>
          <property name="DesktopEntry" type="s" access="read"/>
          <property name="SupportedUriSchemes" type="as" access="read"/>
          <property name="SupportedMimeTypes" type="as" access="read"/>
        </interface>

        """u8.ToArray();

    private static readonly ReadOnlyMemory<byte> PlayerInterfaceXml =
        """
        <interface name="org.mpris.MediaPlayer2.Player">
          <method name="Next"/>
          <method name="Previous"/>
          <method name="Pause"/>
          <method name="PlayPause"/>
          <method name="Stop"/>
          <method name="Play"/>
          <method name="Seek">
            <arg direction="in" name="Offset" type="x"/>
          </method>
          <method name="SetPosition">
            <arg direction="in" name="TrackId" type="o"/>
            <arg direction="in" name="Position" type="x"/>
          </method>
          <method name="OpenUri">
            <arg direction="in" name="Uri" type="s"/>
          </method>
          <signal name="Seeked">
            <arg name="Position" type="x"/>
          </signal>
          <property name="PlaybackStatus" type="s" access="read"/>
          <property name="Metadata" type="a{sv}" access="read"/>
          <property name="Position" type="x" access="read"/>
          <property name="Volume" type="d" access="readwrite"/>
          <property name="Rate" type="d" access="read"/>
          <property name="MinimumRate" type="d" access="read"/>
          <property name="MaximumRate" type="d" access="read"/>
          <property name="CanGoNext" type="b" access="read"/>
          <property name="CanGoPrevious" type="b" access="read"/>
          <property name="CanPlay" type="b" access="read"/>
          <property name="CanPause" type="b" access="read"/>
          <property name="CanSeek" type="b" access="read"/>
          <property name="CanControl" type="b" access="read"/>
        </interface>

        """u8.ToArray();

    private readonly string _busName;
    private readonly string _edition;
    private readonly string _artworkDirectory;
    private readonly Action _raise;
    private readonly Action _quit;

    private readonly object _gate = new();

    private Connection? _connection;
    private bool _disposed;

    // Null means nothing is playing: the surface reports Stopped and no metadata.
    private MediaControlPlaybackStatus? _status;
    private string _title = string.Empty;
    private string _artist = string.Empty;
    private string _album = string.Empty;
    private long _lengthUs;
    private long _positionUs;
    private string? _artUrl;
    private string? _artFilePath;
    private int _trackNumber;
    private int _artworkNumber;
    private bool _hasNext;
    private bool _hasPrevious;
    private double _volume = 1.0;

    public event Action<MediaSessionCommand>? CommandRequested;
    public event Action<double>? SeekRequestedMs;
    public event Action<double>? VolumeRequested;

    internal MprisMediaSession(string edition, Action raise, Action quit)
    {
        _edition = edition;
        _busName = $"{RootInterface}.{edition}";
        _raise = raise;
        _quit = quit;
        // Artwork goes to the per-user runtime directory, which the session owns
        // and clears at logout; the temp directory is the fallback on a system
        // that does not set one.
        var runtimeDirectory = Environment.GetEnvironmentVariable("XDG_RUNTIME_DIR");
        _artworkDirectory = Path.Combine(
            string.IsNullOrEmpty(runtimeDirectory) ? Path.GetTempPath() : runtimeDirectory, edition);
    }

    /// <summary>Go live on the session bus. Separate from construction so the
    /// caller can subscribe to the commands this surface raises before any client
    /// can reach it.</summary>
    internal void Serve() => _ = ServeAsync();

    // MPRIS is a process-wide bus name, not an attachment to a window.
    public void SetWindow(Window? window)
    {
    }

    public void Apply(MediaControlDisplay display)
    {
        Connection? connection;
        VariantValue metadata;
        string status;
        string? droppedArtwork;
        lock (_gate)
        {
            if (_status is null || _title != display.Title || _artist != display.Artist || _album != display.AlbumTitle)
            {
                _trackNumber++;
                _title = display.Title;
                _artist = display.Artist;
                _album = display.AlbumTitle;
                // The track's extent arrives with the first timeline push; until
                // then the metadata carries no length rather than the last one's.
                _lengthUs = 0;
                _positionUs = 0;
            }
            _status = display.Status;
            droppedArtwork = display.Artwork is MediaControlArtwork.Keep ? null : TakeArtworkFile();
            metadata = Metadata();
            status = PlaybackStatusName();
            connection = _connection;
        }

        DeleteArtworkFile(droppedArtwork);
        EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
        {
            ["PlaybackStatus"] = VariantValue.String(status),
            ["Metadata"] = metadata,
        });
    }

    public void SetArtwork(byte[] bytes)
    {
        if (ArtworkExtension(bytes) is not { } extension)
        {
            BaeDiagnostics.Logger.Warning(
                "Cover bytes are not a PNG, JPEG, WebP or GIF; the now-playing surface shows no artwork for this track.");
            return;
        }

        // MPRIS carries artwork as a URL, so the bytes become a file. The name
        // changes with every image so a client never serves the previous track's
        // cover out of its cache.
        string path;
        string? droppedArtwork;
        lock (_gate)
        {
            droppedArtwork = TakeArtworkFile();
            path = Path.Combine(_artworkDirectory, $"cover-{++_artworkNumber}{extension}");
        }
        DeleteArtworkFile(droppedArtwork);

        try
        {
            Directory.CreateDirectory(_artworkDirectory);
            File.WriteAllBytes(path, bytes);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Failed to write the now-playing cover for the MPRIS surface.", exception);
            return;
        }

        Connection? connection;
        VariantValue metadata;
        lock (_gate)
        {
            _artFilePath = path;
            _artUrl = new Uri(path).AbsoluteUri;
            metadata = Metadata();
            connection = _connection;
        }
        EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
        {
            ["Metadata"] = metadata,
        });
    }

    public void SetTimeline(MediaControlTimeline timeline, bool seeked)
    {
        var positionUs = (long)(timeline.PositionMs * 1000UL);
        var lengthUs = (long)(timeline.DurationMs * 1000UL);

        Connection? connection;
        VariantValue metadata = default;
        bool lengthChanged;
        lock (_gate)
        {
            _positionUs = positionUs;
            lengthChanged = _lengthUs != lengthUs;
            _lengthUs = lengthUs;
            if (lengthChanged)
            {
                metadata = Metadata();
            }
            connection = _connection;
        }

        if (lengthChanged)
        {
            EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
            {
                ["Metadata"] = metadata,
            });
        }
        // Position is deliberately absent from the change signal: clients read it
        // on demand and take Seeked as the sign that it jumped.
        if (seeked)
        {
            EmitSeeked(connection, positionUs);
        }
    }

    public void SetCommandAvailability(bool hasNext, bool hasPrevious)
    {
        Connection? connection;
        lock (_gate)
        {
            if (_hasNext == hasNext && _hasPrevious == hasPrevious)
            {
                return;
            }
            _hasNext = hasNext;
            _hasPrevious = hasPrevious;
            connection = _connection;
        }

        EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
        {
            ["CanGoNext"] = VariantValue.Bool(hasNext),
            ["CanGoPrevious"] = VariantValue.Bool(hasPrevious),
        });
    }

    public void SetVolume(double volume)
    {
        Connection? connection;
        lock (_gate)
        {
            if (_volume == volume)
            {
                return;
            }
            _volume = volume;
            connection = _connection;
        }

        EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
        {
            ["Volume"] = VariantValue.Double(volume),
        });
    }

    public void Clear()
    {
        Connection? connection;
        VariantValue metadata;
        string? droppedArtwork;
        lock (_gate)
        {
            _status = null;
            _title = string.Empty;
            _artist = string.Empty;
            _album = string.Empty;
            _lengthUs = 0;
            _positionUs = 0;
            _hasNext = false;
            _hasPrevious = false;
            droppedArtwork = TakeArtworkFile();
            metadata = Metadata();
            connection = _connection;
        }

        DeleteArtworkFile(droppedArtwork);
        EmitPropertiesChanged(connection, PlayerInterface, new Dictionary<string, VariantValue>
        {
            ["PlaybackStatus"] = VariantValue.String("Stopped"),
            ["Metadata"] = metadata,
            ["CanGoNext"] = VariantValue.Bool(false),
            ["CanGoPrevious"] = VariantValue.Bool(false),
        });
    }

    public void Dispose()
    {
        Connection? connection;
        string? droppedArtwork;
        lock (_gate)
        {
            _disposed = true;
            connection = _connection;
            _connection = null;
            droppedArtwork = TakeArtworkFile();
        }

        DeleteArtworkFile(droppedArtwork);
        // Dropping the connection releases the bus name, which is how clients
        // learn the player is gone.
        connection?.Dispose();
    }

    // Connect, take the bus name, then export the player object — in that order,
    // so a client that sees the name always finds the object behind it.
    private async Task ServeAsync()
    {
        Connection? connection = null;
        try
        {
            if (Address.Session is not { Length: > 0 } address)
            {
                BaeDiagnostics.Logger.Error(
                    "No D-Bus session bus address; this run has no OS now-playing surface.");
                return;
            }

            connection = new Connection(address);
            await connection.ConnectAsync();

            var reply = await RequestNameAsync(connection, _busName);
            if (reply is not (RequestNameReplyPrimaryOwner or RequestNameReplyAlreadyOwner))
            {
                BaeDiagnostics.Logger.Error(
                    $"The session bus refused the name {_busName} (reply {reply}); this run has no OS now-playing surface.");
                connection.Dispose();
                return;
            }

            connection.AddMethodHandler(this);
            lock (_gate)
            {
                if (_disposed)
                {
                    connection.Dispose();
                    return;
                }
                _connection = connection;
            }
        }
        catch (Exception exception)
        {
            connection?.Dispose();
            BaeDiagnostics.Logger.Error(
                $"Failed to serve {_busName} on the session bus; this run has no OS now-playing surface.", exception);
        }
    }

    private static Task<uint> RequestNameAsync(Connection connection, string busName)
    {
        using var writer = connection.GetMessageWriter();
        writer.WriteMethodCallHeader(
            destination: Connection.DBusServiceName,
            path: Connection.DBusObjectPath,
            @interface: Connection.DBusInterface,
            member: "RequestName",
            signature: "su");
        writer.WriteString(busName);
        writer.WriteUInt32(RequestNameDoNotQueue);
        return connection.CallMethodAsync(
            writer.CreateMessage(),
            static (Message message, object? state) => message.GetBodyReader().ReadUInt32(),
            null);
    }

    string IMethodHandler.Path => MprisObjectPath;

    // The handlers only touch in-memory state and post their work elsewhere, so
    // running them on the connection's reader keeps the ordering simple.
    bool IMethodHandler.RunMethodHandlerSynchronously(Message message) => true;

    ValueTask IMethodHandler.HandleMethodAsync(MethodContext context)
    {
        if (context.IsDBusIntrospectRequest)
        {
            context.ReplyIntrospectXml([RootInterfaceXml, PlayerInterfaceXml, IntrospectionXml.DBusProperties]);
            return default;
        }

        // Anything left unanswered here gets the library's UnknownMethod reply.
        switch (context.Request.InterfaceAsString)
        {
            case PropertiesInterface:
                HandleProperties(context);
                break;
            case RootInterface:
                HandleRoot(context);
                break;
            case PlayerInterface:
                HandlePlayer(context);
                break;
        }
        return default;
    }

    private void HandleProperties(MethodContext context)
    {
        var request = context.Request;
        var reader = request.GetBodyReader();
        switch (request.MemberAsString)
        {
            case "Get":
                {
                    var (@interface, property) = (reader.ReadString(), reader.ReadString());
                    if (ReadProperty(@interface, property) is not { } value)
                    {
                        context.ReplyError(UnknownPropertyError, $"No property {property} on {@interface}");
                        return;
                    }
                    using var writer = context.CreateReplyWriter("v");
                    writer.WriteVariant(value);
                    context.Reply(writer.CreateMessage());
                    return;
                }
            case "GetAll":
                {
                    var properties = ReadAllProperties(reader.ReadString());
                    using var writer = context.CreateReplyWriter("a{sv}");
                    writer.WriteDictionary(properties);
                    context.Reply(writer.CreateMessage());
                    return;
                }
            case "Set":
                {
                    var (@interface, property) = (reader.ReadString(), reader.ReadString());
                    WriteProperty(context, @interface, property, reader.ReadVariantValue());
                    return;
                }
        }
    }

    private void HandleRoot(MethodContext context)
    {
        switch (context.Request.MemberAsString)
        {
            case "Raise":
                _raise();
                break;
            case "Quit":
                _quit();
                break;
            default:
                return;
        }
        ReplyEmpty(context);
    }

    private void HandlePlayer(MethodContext context)
    {
        var request = context.Request;
        switch (request.MemberAsString)
        {
            case "Next":
                CommandRequested?.Invoke(MediaSessionCommand.Next);
                break;
            case "Previous":
                CommandRequested?.Invoke(MediaSessionCommand.Previous);
                break;
            case "Play":
                CommandRequested?.Invoke(MediaSessionCommand.Play);
                break;
            case "Pause":
                CommandRequested?.Invoke(MediaSessionCommand.Pause);
                break;
            case "Stop":
                CommandRequested?.Invoke(MediaSessionCommand.Stop);
                break;
            case "PlayPause":
                // The surface holds the status for its PlaybackStatus property
                // anyway, so it resolves the toggle here and the app only ever
                // sees an absolute play or pause.
                CommandRequested?.Invoke(IsPlaying() ? MediaSessionCommand.Pause : MediaSessionCommand.Play);
                break;
            case "Seek":
                {
                    // The offset is relative to where the track is now.
                    var reader = request.GetBodyReader();
                    var offsetUs = reader.ReadInt64();
                    long targetUs;
                    lock (_gate)
                    {
                        targetUs = Math.Max(0, _positionUs + offsetUs);
                    }
                    SeekRequestedMs?.Invoke(targetUs / 1000.0);
                    break;
                }
            case "SetPosition":
                {
                    var reader = request.GetBodyReader();
                    var trackId = reader.ReadObjectPathAsString();
                    var positionUs = reader.ReadInt64();
                    // A position for a track that is no longer current is dropped, per
                    // the spec: the client raced a track change.
                    if (IsCurrentTrack(trackId))
                    {
                        SeekRequestedMs?.Invoke(Math.Max(0, positionUs) / 1000.0);
                    }
                    break;
                }
            case "OpenUri":
                BaeDiagnostics.Logger.Warning(
                    "An MPRIS client asked the now-playing surface to open a URI; bae advertises no supported schemes.");
                context.ReplyError(NotSupportedError, "bae does not open URIs from the now-playing surface");
                return;
            default:
                return;
        }
        ReplyEmpty(context);
    }

    // A method call that takes no return value still owes the caller a reply.
    private static void ReplyEmpty(MethodContext context)
    {
        using var writer = context.CreateReplyWriter(null);
        context.Reply(writer.CreateMessage());
    }

    private VariantValue? ReadProperty(string @interface, string property)
    {
        lock (_gate)
        {
            return Property(@interface, property);
        }
    }

    private Dictionary<string, VariantValue> ReadAllProperties(string @interface)
    {
        var names = @interface switch
        {
            RootInterface => RootPropertyNames,
            PlayerInterface => PlayerPropertyNames,
            _ => NoStrings,
        };
        var all = new Dictionary<string, VariantValue>(names.Length);
        lock (_gate)
        {
            foreach (var name in names)
            {
                if (Property(@interface, name) is { } value)
                {
                    all[name] = value;
                }
            }
        }
        return all;
    }

    private void WriteProperty(MethodContext context, string @interface, string property, VariantValue value)
    {
        if (@interface == PlayerInterface && property == "Volume")
        {
            if (value.Type != VariantValueType.Double)
            {
                context.ReplyError(InvalidArgsError, "Volume takes a double");
                return;
            }
            VolumeRequested?.Invoke(Math.Clamp(value.GetDouble(), 0.0, 1.0));
            ReplyEmpty(context);
            return;
        }

        context.ReplyError(
            ReadProperty(@interface, property) is null ? UnknownPropertyError : ReadOnlyPropertyError,
            $"{@interface}.{property} cannot be set");
    }

    // Every exported property, resolved against the published state. Callers hold
    // the gate, so one GetAll reply is one consistent snapshot.
    private VariantValue? Property(string @interface, string property) =>
        @interface switch
        {
            RootInterface => property switch
            {
                "CanQuit" => (VariantValue?)VariantValue.Bool(true),
                "CanRaise" => VariantValue.Bool(true),
                "HasTrackList" => VariantValue.Bool(false),
                "Identity" => VariantValue.String(_edition),
                "DesktopEntry" => VariantValue.String(_edition),
                "SupportedUriSchemes" => VariantValue.Array(NoStrings),
                "SupportedMimeTypes" => VariantValue.Array(NoStrings),
                _ => null,
            },
            PlayerInterface => property switch
            {
                "PlaybackStatus" => (VariantValue?)VariantValue.String(PlaybackStatusName()),
                "Metadata" => Metadata(),
                "Position" => VariantValue.Int64(_positionUs),
                "Volume" => VariantValue.Double(_volume),
                // Playback runs at the recorded speed and nothing exposes a rate
                // control, so the range is a single point.
                "Rate" or "MinimumRate" or "MaximumRate" => VariantValue.Double(1.0),
                "CanGoNext" => VariantValue.Bool(_hasNext),
                "CanGoPrevious" => VariantValue.Bool(_hasPrevious),
                "CanPlay" => VariantValue.Bool(true),
                "CanPause" => VariantValue.Bool(true),
                "CanSeek" => VariantValue.Bool(true),
                "CanControl" => VariantValue.Bool(true),
                _ => null,
            },
            _ => null,
        };

    // The current track's metadata, or an empty map when nothing plays. Callers
    // hold the gate.
    private VariantValue Metadata()
    {
        var metadata = new Dict<string, VariantValue>();
        if (_status is null)
        {
            return metadata.AsVariantValue();
        }

        metadata["mpris:trackid"] = VariantValue.ObjectPath(new ObjectPath($"{TrackIdPrefix}{_trackNumber}"));
        if (_lengthUs > 0)
        {
            metadata["mpris:length"] = VariantValue.Int64(_lengthUs);
        }
        if (_artUrl is { } artUrl)
        {
            metadata["mpris:artUrl"] = VariantValue.String(artUrl);
        }
        metadata["xesam:title"] = VariantValue.String(_title);
        // Preview clips carry only a file name, so artist and album are left out
        // rather than published blank.
        if (_artist.Length > 0)
        {
            metadata["xesam:artist"] = VariantValue.Array(new[] { _artist });
        }
        if (_album.Length > 0)
        {
            metadata["xesam:album"] = VariantValue.String(_album);
        }
        return metadata.AsVariantValue();
    }

    // MPRIS has no changing state; a resolved loading target is on its way to
    // playing. Callers hold the gate.
    private string PlaybackStatusName() =>
        _status switch
        {
            MediaControlPlaybackStatus.Playing or MediaControlPlaybackStatus.Changing => "Playing",
            MediaControlPlaybackStatus.Paused => "Paused",
            _ => "Stopped",
        };

    private bool IsPlaying()
    {
        lock (_gate)
        {
            return _status is MediaControlPlaybackStatus.Playing or MediaControlPlaybackStatus.Changing;
        }
    }

    private bool IsCurrentTrack(string trackId)
    {
        lock (_gate)
        {
            return _status is not null && trackId == $"{TrackIdPrefix}{_trackNumber}";
        }
    }

    // Forgets the published artwork and hands back the file that backed it, for
    // the caller to delete outside the gate. Callers hold the gate.
    private string? TakeArtworkFile()
    {
        var path = _artFilePath;
        _artFilePath = null;
        _artUrl = null;
        return path;
    }

    private static void DeleteArtworkFile(string? path)
    {
        if (path is null)
        {
            return;
        }

        try
        {
            File.Delete(path);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Debug($"Could not delete the previous now-playing cover {path}.", exception);
        }
    }

    // The image formats MPRIS clients render. Bytes that match none of them get no
    // artwork at all rather than a URL clients cannot decode.
    private static string? ArtworkExtension(byte[] bytes) =>
        bytes switch
        {
            [0x89, (byte)'P', (byte)'N', (byte)'G', ..] => ".png",
            [0xFF, 0xD8, 0xFF, ..] => ".jpg",
            [(byte)'R', (byte)'I', (byte)'F', (byte)'F', _, _, _, _, (byte)'W', (byte)'E', (byte)'B', (byte)'P', ..] => ".webp",
            [(byte)'G', (byte)'I', (byte)'F', (byte)'8', ..] => ".gif",
            _ => null,
        };

    private static void EmitPropertiesChanged(
        Connection? connection, string @interface, Dictionary<string, VariantValue> changed)
    {
        if (connection is null)
        {
            return;
        }

        using var writer = connection.GetMessageWriter();
        writer.WriteSignalHeader(
            path: MprisObjectPath,
            @interface: PropertiesInterface,
            member: "PropertiesChanged",
            signature: "sa{sv}as");
        writer.WriteString(@interface);
        writer.WriteDictionary(changed);
        writer.WriteArray(NoStrings);
        connection.TrySendMessage(writer.CreateMessage());
    }

    private static void EmitSeeked(Connection? connection, long positionUs)
    {
        if (connection is null)
        {
            return;
        }

        using var writer = connection.GetMessageWriter();
        writer.WriteSignalHeader(
            path: MprisObjectPath,
            @interface: PlayerInterface,
            member: "Seeked",
            signature: "x");
        writer.WriteInt64(positionUs);
        connection.TrySendMessage(writer.CreateMessage());
    }
}
