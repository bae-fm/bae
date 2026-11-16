package fm.bae.app

import android.content.Context
import uniffi.bae_bridge.BridgeFileKind

// What kind of file a release file is, in the person's language. This file names file kinds the same way as the
// macOS file details view.

/** The kind in words: an audio or image file names its format, the other kinds have a name each. */
fun Context.fileKindText(kind: BridgeFileKind): String =
    when (kind) {
        is BridgeFileKind.Audio -> getString(R.string.file_kind_audio, kind.format)
        is BridgeFileKind.Image -> getString(R.string.file_kind_image, kind.format)
        BridgeFileKind.DamagedImage -> getString(R.string.file_kind_damaged_image)
        BridgeFileKind.CueSheet -> getString(R.string.file_kind_cue_sheet)
        BridgeFileKind.RipLog -> getString(R.string.file_kind_rip_log)
        BridgeFileKind.AccurateRipReport -> getString(R.string.file_kind_accuraterip_report)
        BridgeFileKind.Playlist -> getString(R.string.file_kind_playlist)
        BridgeFileKind.Text -> getString(R.string.file_kind_text)
        BridgeFileKind.Pdf -> getString(R.string.file_kind_pdf)
        BridgeFileKind.Other -> getString(R.string.file_kind_other)
    }
