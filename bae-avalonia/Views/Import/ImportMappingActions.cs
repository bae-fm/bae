using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What the mapping table's rows call back into. One table, one set of actions:
/// the left half's decisions about the folder, and the right half's about the
/// tracklist being committed.
/// </summary>
/// <param name="SetRole">Put a file in a role, or put it back: the file's id,
/// then the choice. Core persists it, and the table is re-read because a role
/// change is a different set of rows.</param>
/// <param name="BindSheet">Name the audio a track sheet describes: the sheet's
/// file id, then the audio's, or null to leave the sheet describing
/// nothing.</param>
/// <param name="SetSheetDisc">Say which disc of the release a track sheet's
/// entries are, or take them out of the tracklist: the sheet's file id, then the
/// assignment.</param>
/// <param name="OpenDocument">Open a document (a log, a text file, a track
/// sheet) in the viewer: the file's name, then its path on disk.</param>
/// <param name="OpenImages">Open the folder's images in the lightbox: the
/// gallery's images, then the path of the one that was clicked.</param>
/// <param name="Preview">Audition a row's exact source window.</param>
/// <param name="StopPreview">Stop whatever is auditioning.</param>
/// <param name="EditTrack">Write a row's edited track back onto the row that
/// commits it.</param>
/// <param name="SetTrackArtists">Apply one artist choice to the named track
/// rows as one operation.</param>
/// <param name="ChooseFile">Point a row at one of the folder's audio units: the
/// row's track id, then the unit.</param>
/// <param name="Drop">Remove a row from the import entirely — a track the
/// release names that this folder has nothing for.</param>
/// <param name="Exclude">Take a file out of the tracklist by id. Persisted: it
/// is a fact about the folder, so it survives re-picking a release.</param>
internal sealed record ImportMappingActions(
    Action<string, BridgeFileRoleChoice> SetRole,
    Action<string, string?> BindSheet,
    Action<string, BridgeSheetDisc> SetSheetDisc,
    Action<string, string> OpenDocument,
    Action<IReadOnlyList<BridgeMappingImage>, string> OpenImages,
    Action<BridgePreviewTarget> Preview,
    Action StopPreview,
    Action<BridgeRawTrackEdit> EditTrack,
    Action<IReadOnlyList<string>, BridgeTrackArtistAssignments> SetTrackArtists,
    Action<string, BridgeAudioFile> ChooseFile,
    Action<string> Drop,
    Action<string> Exclude);
