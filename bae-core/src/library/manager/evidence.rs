//! Open the files behind release facts, retaining every sighting of a value.

use super::{LibraryError, LibraryManager};
use crate::import::{MarkKind, ReleaseMark, Verification};
use crate::signals::SignalOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceSubject {
    Candidate { key: String },
    Release { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceSelection {
    Mark {
        kind: MarkKind,
        value: String,
        origin: SignalOrigin,
    },
    Verification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceContent {
    Document { name: String, text: String },
    Image { name: String, bytes: Vec<u8> },
    Reveal { path: String },
}

impl LibraryManager {
    pub async fn read_evidence(
        &self,
        subject: &EvidenceSubject,
        selection: &EvidenceSelection,
    ) -> Result<Vec<EvidenceContent>, LibraryError> {
        let contents = match subject {
            EvidenceSubject::Candidate { key } => {
                let detail = self.load_import_candidate(key).await?.ok_or_else(|| {
                    LibraryError::Import(format!("Candidate {key} no longer exists"))
                })?;
                let signals = detail.signals.as_ref().ok_or_else(|| {
                    LibraryError::Import(format!("Candidate {key} has no extracted evidence"))
                })?;
                match selection {
                    EvidenceSelection::Mark {
                        origin: SignalOrigin::FolderName,
                        ..
                    } => detail
                        .candidate
                        .source_folders()
                        .into_iter()
                        .map(|path| EvidenceContent::Reveal {
                            path: path.to_string_lossy().into_owned(),
                        })
                        .collect(),
                    EvidenceSelection::Mark { origin, .. } => {
                        let marks = ReleaseMark::of_signals(signals, &detail.lookup_choices);
                        let names = mark_files(&marks, selection)?;
                        let mut contents = Vec::new();
                        for name in names {
                            let file = detail
                                .candidate
                                .files()
                                .release_files()
                                .find(|file| file.relative_path == name)
                                .ok_or_else(|| {
                                    LibraryError::Import(format!(
                                        "Evidence file {name} is no longer in {key}"
                                    ))
                                })?;
                            contents.push(if *origin == SignalOrigin::Filename {
                                EvidenceContent::Reveal {
                                    path: file.path.to_string_lossy().into_owned(),
                                }
                            } else {
                                content(name, *origin, tokio::fs::read(&file.path).await?)
                            });
                        }
                        contents
                    }
                    EvidenceSelection::Verification => {
                        let verification = signals
                            .verification
                            .as_ref()
                            .ok_or_else(missing_verification)?;
                        let mut contents = Vec::new();
                        for file in detail
                            .candidate
                            .files()
                            .documents()
                            .filter(|file| is_log(&file.relative_path))
                        {
                            let text = crate::text_encoding::decode_text(
                                &tokio::fs::read(&file.path).await?,
                            )
                            .text;
                            if log_confirms(&text, verification) {
                                contents.push(EvidenceContent::Document {
                                    name: file.relative_path.clone(),
                                    text,
                                });
                            }
                        }
                        contents
                    }
                }
            }
            EvidenceSubject::Release { id } => {
                let files = self.get_files_for_release(id).await?;
                match selection {
                    EvidenceSelection::Mark {
                        origin: SignalOrigin::FolderName,
                        ..
                    } => {
                        let mut roots = Vec::new();
                        for file in &files {
                            if let Some(mut path) = self.file_local_path(&file.id).await? {
                                if !path.ends_with(&file.original_filename) {
                                    return Err(LibraryError::Storage(format!(
                                        "The original source folder for {} is no longer available",
                                        file.original_filename
                                    )));
                                }
                                for _ in std::path::Path::new(&file.original_filename).components()
                                {
                                    path.pop();
                                }
                                let root = EvidenceContent::Reveal {
                                    path: path.to_string_lossy().into_owned(),
                                };
                                if !roots.contains(&root) {
                                    roots.push(root);
                                }
                            }
                        }
                        roots
                    }
                    EvidenceSelection::Mark { origin, .. } => {
                        let marks = self.database.get_release_marks(id).await?;
                        let names = mark_files(&marks, selection)?;
                        let mut contents = Vec::new();
                        for name in names {
                            let file = files
                                .iter()
                                .find(|file| file.original_filename == name)
                                .ok_or_else(|| {
                                    LibraryError::Storage(format!(
                                        "Evidence file {name} is not in release {id}"
                                    ))
                                })?;
                            contents.push(if *origin == SignalOrigin::Filename {
                                let path =
                                    self.file_local_path(&file.id).await?.ok_or_else(|| {
                                        LibraryError::Storage(format!(
                                            "{name} has no local file to reveal"
                                        ))
                                    })?;
                                EvidenceContent::Reveal {
                                    path: path.to_string_lossy().into_owned(),
                                }
                            } else {
                                content(name, *origin, self.read_release_blob(file).await?)
                            });
                        }
                        contents
                    }
                    EvidenceSelection::Verification => {
                        let detail = self.find_release_detail(id).await?.ok_or_else(|| {
                            LibraryError::Storage(format!("Release {id} no longer exists"))
                        })?;
                        let verification = detail
                            .verification
                            .as_ref()
                            .ok_or_else(missing_verification)?;
                        let mut contents = Vec::new();
                        for file in files.iter().filter(|file| is_log(&file.original_filename)) {
                            let text = crate::text_encoding::decode_text(
                                &self.read_release_blob(file).await?,
                            )
                            .text;
                            if log_confirms(&text, verification) {
                                contents.push(EvidenceContent::Document {
                                    name: file.original_filename.clone(),
                                    text,
                                });
                            }
                        }
                        contents
                    }
                }
            }
        };
        if contents.is_empty() {
            return Err(LibraryError::Storage(
                "The source of this evidence is no longer available".into(),
            ));
        }
        Ok(contents)
    }
}

fn mark_files(
    marks: &[ReleaseMark],
    selection: &EvidenceSelection,
) -> Result<Vec<String>, LibraryError> {
    let EvidenceSelection::Mark {
        kind,
        value,
        origin,
    } = selection
    else {
        unreachable!("only marks have sighting paths")
    };
    let mut names = Vec::new();
    for mark in marks.iter().filter(|mark| {
        mark.kind == *kind
            && kind.same_value(&mark.sighting.value, value)
            && mark.sighting.origin == *origin
    }) {
        let name = mark.sighting.origin_path.as_ref().ok_or_else(|| {
            LibraryError::Storage(format!("The source filename for {value} was not recorded"))
        })?;
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    Ok(names)
}

fn content(name: String, origin: SignalOrigin, bytes: Vec<u8>) -> EvidenceContent {
    match origin {
        SignalOrigin::Artwork => EvidenceContent::Image { name, bytes },
        SignalOrigin::DiscToc | SignalOrigin::CueSheet | SignalOrigin::TextFile => {
            EvidenceContent::Document {
                name,
                text: crate::text_encoding::decode_text(&bytes).text,
            }
        }
        SignalOrigin::Filename | SignalOrigin::FolderName => {
            unreachable!("names reveal their source paths")
        }
    }
}

fn is_log(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("log"))
}

fn missing_verification() -> LibraryError {
    LibraryError::Storage("This release has no rip verification".into())
}

/// Stored verification contains track results, not a filename. Match the whole
/// result against the release's logs; the disc-ID log may be a different file.
/// Multiple logs declaring the same result are all evidence for that result.
fn log_confirms(text: &str, verification: &Verification) -> bool {
    match crate::import::rip_log::parse_rip_log(text) {
        Ok(log) => Verification::of(&log) == *verification,
        Err(error) => {
            tracing::debug!(%error, "log does not contain a recognized rip result");
            false
        }
    }
}
