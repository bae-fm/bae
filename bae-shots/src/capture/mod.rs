//! Running a platform's harness and collecting the shots it writes.

pub(crate) mod android;
pub(crate) mod mac;

use crate::names::{Name, Platform, Prefix};
use crate::shots::Shot;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub(crate) enum CaptureError {
    #[error("cannot start {step}: {source}")]
    Start { step: String, source: std::io::Error },
    #[error("{step} failed: {status}")]
    Failed { step: String, status: std::process::ExitStatus },
    #[error("{step} printed output that is not text: {source}")]
    NotText { step: String, source: std::string::FromUtf8Error },
    #[error("cannot prepare {0}: {1}")]
    Directory(PathBuf, std::io::Error),
    #[error("the prefix '{0}' selects no declared shot")]
    UnknownPrefix(String),
    #[error("no declared {0:?} shot is selected")]
    NothingSelected(Platform),
    #[error("the capture is incomplete:\n{}", .0.join("\n"))]
    Incomplete(Vec<String>),
}

/// One program run with arguments, never a shell string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) name: String,
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) env: Vec<(String, String)>,
}

impl Step {
    pub(crate) fn new(name: &str, cwd: &Path, program: &str, args: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            cwd: cwd.to_path_buf(),
            env: Vec::new(),
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args).current_dir(&self.cwd).envs(self.env.iter().map(|(k, v)| (k, v)));
        command
    }

    /// Runs the step with this process's output and fails naming it.
    pub(crate) fn run(&self) -> Result<(), CaptureError> {
        eprintln!("bae-shots: {}", self.name);
        let status = self
            .command()
            .status()
            .map_err(|source| CaptureError::Start { step: self.name.clone(), source })?;
        if status.success() {
            Ok(())
        } else {
            Err(CaptureError::Failed { step: self.name.clone(), status })
        }
    }

    /// Runs the step and returns what it printed, trimmed.
    pub(crate) fn output(&self) -> Result<String, CaptureError> {
        let output = self
            .command()
            .output()
            .map_err(|source| CaptureError::Start { step: self.name.clone(), source })?;
        if !output.status.success() {
            return Err(CaptureError::Failed { step: self.name.clone(), status: output.status });
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|source| CaptureError::NotText { step: self.name.clone(), source })?;
        Ok(text.trim().to_string())
    }
}

/// The declared shots of `platform` a run takes: all of them when no prefix is
/// given, else those a prefix of that platform starts.
pub(crate) fn selected(
    declared: &[Name],
    platform: Platform,
    prefixes: &[Prefix],
) -> Result<Vec<Name>, CaptureError> {
    for prefix in prefixes {
        if !declared.iter().any(|name| prefix.matches(name)) {
            return Err(CaptureError::UnknownPrefix(prefix.as_str().to_string()));
        }
    }
    let selected: Vec<Name> = declared
        .iter()
        .filter(|name| name.platform() == platform)
        .filter(|name| prefixes.is_empty() || prefixes.iter().any(|p| p.matches(name)))
        .cloned()
        .collect();
    if selected.is_empty() {
        return Err(CaptureError::NothingSelected(platform));
    }
    Ok(selected)
}

const EXTENSIONS: [&str; 2] = ["png", "json"];

/// Empties `out` of `platform`'s shot files, so a capture starts from nothing
/// and the completeness check sees only what this run wrote. The folder is
/// shared by every branch built in a checkout; what another branch left behind
/// would otherwise read as undeclared.
pub(crate) fn clear(out: &Path, platform: Platform) -> Result<(), CaptureError> {
    std::fs::create_dir_all(out).map_err(|e| CaptureError::Directory(out.to_path_buf(), e))?;
    let entries =
        std::fs::read_dir(out).map_err(|e| CaptureError::Directory(out.to_path_buf(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| CaptureError::Directory(out.to_path_buf(), e))?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if Name::from_file_name(platform, &file).is_some() {
            std::fs::remove_file(entry.path())
                .map_err(|e| CaptureError::Directory(entry.path(), e))?;
        }
    }
    Ok(())
}

/// Compares what a capture wrote with what it was to write, both ways: every
/// selected shot wrote its PNG and its element list, which read back and agree
/// with each other, and `out` holds no shot file of `platform` that no harness
/// declares.
pub(crate) fn check_complete(
    out: &Path,
    platform: Platform,
    selected: &[Name],
    declared: &[Name],
) -> Result<(), CaptureError> {
    let mut problems = Vec::new();
    for name in selected {
        let mut written = true;
        for extension in EXTENSIONS {
            let file = name.file_name(extension);
            if !out.join(&file).is_file() {
                problems.push(format!("missing: {file} was not written"));
                written = false;
            }
        }
        if !written {
            continue;
        }
        match Shot::load(out, name) {
            Ok(shot) => problems.extend(
                shot.problems(name)
                    .into_iter()
                    .map(|problem| format!("inconsistent: {name}: {problem}")),
            ),
            Err(e) => problems.push(format!("unreadable: {name}: {e}")),
        }
    }
    let entries =
        std::fs::read_dir(out).map_err(|e| CaptureError::Directory(out.to_path_buf(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| CaptureError::Directory(out.to_path_buf(), e))?;
        let file = entry.file_name().to_string_lossy().into_owned();
        match Name::from_file_name(platform, &file) {
            None => {}
            Some(Ok(name)) if declared.contains(&name) => {}
            Some(_) => problems.push(format!("undeclared: {file} is no declared shot")),
        }
    }
    problems.sort();
    if problems.is_empty() {
        Ok(())
    } else {
        Err(CaptureError::Incomplete(problems))
    }
}

#[cfg(test)]
#[path = "capture_tests.rs"]
mod tests;
