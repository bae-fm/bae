//! The gain a track plays at, from the loudness it and its release were
//! measured at.

use bae_preferences::ReplayGainMode;
use bae_records::ResolvedTrackAudio;

/// Target playback loudness the replay gain aims each track/album at, in LUFS.
/// -18 LUFS is a common reference for quiet-listening normalization.
const REPLAY_GAIN_TARGET_LUFS: f64 = -18.0;

/// Linear playback gain for `audio` under `mode`. `1.0` = no change.
///
/// The gain is a view of (stored measurements, mode, target) — never stored.
/// `Off` plays at unity. `Track`/`Album` pick that level's `(loudness, peak)`,
/// falling back to the other level when the preferred one wasn't measured,
/// and to unity when neither was (NULL measurements, or a silent track). For
/// the chosen level the gain brings the measured loudness to the target, then
/// is capped at `1.0/peak` so a boosted track can't clip.
pub fn replay_gain_linear(audio: &ResolvedTrackAudio, mode: ReplayGainMode) -> f32 {
    let track = audio.track_loudness_lufs.zip(audio.track_peak_linear);
    let album = audio.album_loudness_lufs.zip(audio.album_peak_linear);

    let chosen = match mode {
        ReplayGainMode::Off => None,
        ReplayGainMode::Track => track.or(album),
        ReplayGainMode::Album => album.or(track),
    };

    let Some((loudness_lufs, peak_linear)) = chosen else {
        return 1.0;
    };

    let gain = 10f64.powf((REPLAY_GAIN_TARGET_LUFS - loudness_lufs) / 20.0);
    // Cap the gain so the loudest true-peak sample can't exceed full scale.
    // A non-positive peak (no usable peak) imposes no cap.
    let max_safe = if peak_linear > 0.0 { 1.0 / peak_linear } else { f64::INFINITY };
    gain.min(max_safe) as f32
}
