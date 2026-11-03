//! The audio each `FILE` reference of a track sheet names, as far as the
//! sheet's folder and the folders above it can say.

use super::*;

// ── Candidate file index ────────────────────────────────────────────────────

/// Resolve every audio path a CUE names. Each `FILE` uses the same rule
/// ([`resolve_cue_audio_path`]), and every reference must resolve to a
/// different file.
pub(crate) fn resolve_cue_audio_paths<'sheet, 'audio>(
    cue_path: &Path,
    sheet: &'sheet CueSheet,
    audio_files: &'audio [PathBuf],
) -> Option<Vec<(&'sheet str, &'audio PathBuf)>> {
    let references = sheet.audio_file_references();
    if references.is_empty() {
        return None;
    }
    let mut resolved = Vec::with_capacity(references.len());
    let mut used = BTreeSet::new();
    for reference in references {
        let audio = resolve_cue_audio_path(cue_path, reference, audio_files)?;
        if !used.insert(audio.as_path()) {
            debug!("CUE {:?} resolves more than one FILE reference to {:?}", cue_path, audio);
            return None;
        }
        resolved.push((reference, audio));
    }
    Some(resolved)
}

/// The audio one `FILE` reference names, as far as the folder can say.
///
/// A sheet's reference is the path the ripper wrote, which is the path on the
/// ripper's machine: folders that are not here, `\\` where this system reads
/// `/`, the WAV the audio was encoded from rather than the FLAC it became, a
/// name in another case on a system that ignores case, or the same letters in
/// another Unicode normalization than the file system stored. So the
/// reference is read as its components and tried against the sheet's own
/// folder first — a reference is relative to its sheet by convention, which is
/// what tells `CD1/01.wav` from `CD2/01.wav` — then against each folder above
/// it. In each folder it is tried from the whole path down to the bare file
/// name, dropping one leading folder at a time. At each length the literal
/// path wins; then the one audio file whose path reads the same once case and
/// normalization are set aside; then the one audio file beside it whose stem
/// reads the same that way, whatever its extension. The first try that names
/// exactly one file is the answer; one that names several names none.
pub(super) fn resolve_cue_audio_path<'a>(
    cue_path: &Path,
    file_reference: &str,
    audio_files: &'a [PathBuf],
) -> Option<&'a PathBuf> {
    let cue_dir = cue_path.parent()?;
    let components: Vec<&str> = file_reference
        .split(['/', '\\'])
        .filter(|component| !component.is_empty() && *component != ".")
        .collect();
    let file_name = *components.last()?;
    let file_stem = spelled(Path::new(file_name).file_stem()?.to_str()?);
    for base in cue_dir.ancestors() {
        for start in 0..components.len() {
            let mut referenced = base.to_path_buf();
            for component in &components[start..] {
                referenced.push(component);
            }
            if let Some(exact) = audio_files.iter().find(|path| **path == referenced) {
                return Some(exact);
            }
            let reference_dir = spelled_path(referenced.parent()?);
            let same_path = spelled_path(&referenced);
            let same_name = audio_files.iter().filter(|path| spelled_path(path) == same_path);
            let same_stem = audio_files.iter().filter(|path| {
                path.parent().is_some_and(|dir| spelled_path(dir) == reference_dir)
                    && path.file_stem().is_some_and(|stem| {
                        spelled(&coven::path_text(Path::new(stem))) == file_stem
                    })
            });
            for mut matches in
                [Box::new(same_name) as Box<dyn Iterator<Item = &'a PathBuf>>, Box::new(same_stem)]
            {
                if let Some(matched) = matches.next() {
                    if matches.next().is_some() {
                        debug!(
                            "CUE {cue_path:?} names {referenced:?}, which more than one audio file answers"
                        );
                        return None;
                    }
                    return Some(matched);
                }
            }
        }
    }
    None
}

/// How a name reads once case and Unicode normalization are set aside.
fn spelled(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    name.nfc().flat_map(char::to_lowercase).collect()
}

/// How a path reads once case and Unicode normalization are set aside, one
/// component at a time. Comparing components rather than the path's text keeps
/// `a\b` and `a/b` the same path on Windows, where a pushed component is
/// joined with `\` and a path read from elsewhere may carry `/`.
fn spelled_path(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| spelled(&coven::path_text(Path::new(component.as_os_str()))))
        .collect()
}

#[cfg(test)]
#[path = "sheet_references_tests.rs"]
mod tests;
