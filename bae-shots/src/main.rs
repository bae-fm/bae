//! bae-shots: takes the shots of the apps and checks them against the design
//! boards.

mod capture;
mod compare;
mod declared;
mod design;
mod lint;
mod lists;
mod names;
mod record;
mod render;
mod report;
mod shots;
mod verify;

use clap::{Args, Parser, Subcommand};
use names::{Name, Platform, Prefix};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// The branch a local run marks its pairs new or changed against.
const LOCAL_BASE: &str = "origin/main";

#[derive(Parser)]
#[command(name = "bae-shots")]
struct Cli {
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    /// List the shots the harnesses declare that have no board.
    Lint,
    /// Check the canvas runtime copy against its record and the canvas's.
    Runtime,
    /// Write the record the last `run` made (shots/record.toml) to
    /// design/shots.toml, for whoever commits it.
    Record,
    /// Take the shots into shots/<platform>/ and check them. Only pairs new or
    /// changed against origin/main's record fail the run; the rest are listed.
    Run(Selection),
    /// Check shots already taken against their boards.
    Check {
        /// The directory holding every platform's PNGs and element lists.
        #[arg(long)]
        shots: PathBuf,
        /// The directory the rendered boards are written to.
        #[arg(long)]
        out: PathBuf,
        /// A git ref whose record the pairs are marked new or changed against.
        #[arg(long)]
        base: Option<String>,
        /// The commit the shots were taken at, for the report.
        #[arg(long)]
        sha: String,
    },
    /// Take the shots into a directory.
    Capture {
        #[command(flatten)]
        selection: Selection,
        /// The directory the PNGs and element lists are written to.
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Args)]
struct Selection {
    /// A platform to take shots on (mac, android); repeat for several. With
    /// none, the platforms of the --select prefixes, or all of them.
    #[arg(long = "platform", value_parser = parse_platform)]
    platforms: Vec<Platform>,
    /// A board-name prefix: `mac.welcome.` is the Mac welcome boards,
    /// `mac.component.row.` one component's states. Repeat for several. With
    /// none, every shot the platform declares.
    #[arg(long = "select", value_parser = Prefix::parse)]
    prefixes: Vec<Prefix>,
    /// Also take the shots shown in their windows, which bring the app to the
    /// front and take the focus.
    #[arg(long)]
    windows: bool,
}

fn parse_platform(text: &str) -> Result<Platform, String> {
    Platform::parse(text).ok_or_else(|| "a platform is mac or android".to_string())
}

impl Selection {
    fn platforms(&self) -> Vec<Platform> {
        let mut platforms = if !self.platforms.is_empty() {
            self.platforms.clone()
        } else if !self.prefixes.is_empty() {
            self.prefixes.iter().map(Prefix::platform).collect()
        } else {
            vec![Platform::Mac, Platform::Android]
        };
        platforms.sort();
        platforms.dedup();
        platforms
    }
}

/// The checkout the command runs in: the nearest directory above the working
/// directory that holds the design system's components.
fn repository_root() -> Result<PathBuf, String> {
    let start = std::env::current_dir().map_err(|e| e.to_string())?;
    start
        .ancestors()
        .find(|d| d.join("design/system/components").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("no design/system/components above {}", start.display()))
}

fn capture(
    root: &Path,
    selection: &Selection,
    out_for: impl Fn(Platform) -> PathBuf,
) -> Result<(), String> {
    let declared = declared::read(root).map_err(|e| e.to_string())?;
    let names: Vec<Name> = declared.iter().map(|d| d.name.clone()).collect();
    for platform in selection.platforms() {
        let out = std::path::absolute(out_for(platform)).map_err(|e| e.to_string())?;
        match platform {
            Platform::Mac => {
                capture::mac::capture(root, &declared, &selection.prefixes, selection.windows, &out)
                    .map(drop)
            }
            Platform::Android => capture::android::capture(root, &names, &selection.prefixes, &out),
        }
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The declared shots the selection takes on every platform it names, and
/// those it leaves out because they need a window the run was not given.
fn selected(root: &Path, selection: &Selection) -> Result<capture::Taken, String> {
    let declared = declared::read(root).map_err(|e| e.to_string())?;
    let mut all = capture::Taken { names: Vec::new(), not_taken: Vec::new() };
    for platform in selection.platforms() {
        let taken = capture::taken(&declared, platform, &selection.prefixes, selection.windows)
            .map_err(|e| e.to_string())?;
        all.names.extend(taken.names);
        all.not_taken.extend(taken.not_taken);
    }
    Ok(all)
}

/// The commit at the checkout's head.
fn commit(root: &Path) -> Result<String, String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--short=9", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err("git could not name the head commit".to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn header(commit: String, base: Option<String>, outcome: &verify::Outcome) -> report::Header {
    report::Header {
        commit,
        base,
        runtime_published_at: outcome.runtime_published_at.clone(),
        chrome: format!("Chrome for Testing {}", render::chrome::PINNED),
    }
}

/// Prints the outcome, writes the report into `report_dir` and, in CI, the
/// step summary. True when nothing fails.
fn finish(
    outcome: &verify::Outcome,
    header: &report::Header,
    report_dir: &Path,
) -> Result<bool, String> {
    verify::print(outcome);
    report::write(report_dir, outcome, header)?;
    eprintln!("bae-shots: report at {}", report_dir.join("index.html").display());
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&summary)
            .map_err(|e| format!("cannot open the step summary: {e}"))?;
        file.write_all(report::summary(outcome, header).as_bytes())
            .map_err(|e| format!("cannot write the step summary: {e}"))?;
    }
    Ok(outcome.passed())
}

fn run(action: Action) -> Result<bool, String> {
    let root = repository_root()?;
    match action {
        Action::Lint => {
            let declared = declared::read(&root).map_err(|e| e.to_string())?;
            let record =
                record::Record::load(&root.join(record::FILE)).map_err(|e| e.to_string())?;
            let problems = lint::problems(&root, &declared, &record);
            for problem in &problems {
                eprintln!("{}", problem.sentence);
            }
            Ok(problems.is_empty())
        }
        Action::Run(selection) => {
            capture(&root, &selection, |p| root.join("shots").join(p.word()))?;
            let taken = selected(&root, &selection)?;
            let declared = declared::read(&root).map_err(|e| e.to_string())?;
            let outcome = verify::verify(&verify::Verification {
                root: &root,
                names: &taken.names,
                not_taken: &taken.not_taken,
                declared: &declared,
                shots_for: &|p| root.join("shots").join(p.word()),
                out: &root.join("shots"),
                base: Some(LOCAL_BASE),
                results: Some(&root.join("shots").join(record::RESULTS)),
                gate: verify::Gate::NewOrChanged,
            })?;
            let header = header(commit(&root)?, Some(LOCAL_BASE.to_string()), &outcome);
            finish(&outcome, &header, &root.join("shots/report"))
        }
        Action::Check { shots, out, base, sha } => {
            let declared = declared::read(&root).map_err(|e| e.to_string())?;
            let names: Vec<Name> = declared.iter().map(|d| d.name.clone()).collect();
            for platform in [Platform::Mac, Platform::Android] {
                let wanted: Vec<Name> =
                    names.iter().filter(|n| n.platform() == platform).cloned().collect();
                capture::check_complete(&shots, platform, &wanted, &names)
                    .map_err(|e| e.to_string())?;
            }
            let outcome = verify::verify(&verify::Verification {
                root: &root,
                names: &names,
                not_taken: &[],
                declared: &declared,
                shots_for: &|_| shots.clone(),
                out: &out,
                base: base.as_deref(),
                results: None,
                gate: verify::Gate::EveryPair,
            })?;
            let header = header(sha, base, &outcome);
            finish(&outcome, &header, &out)
        }
        Action::Record => {
            record::Record::promote(&root.join("shots").join(record::RESULTS), &root)
                .map_err(|e| e.to_string())?;
            eprintln!("bae-shots: wrote {}", record::FILE);
            Ok(true)
        }
        Action::Capture { selection, out } => {
            capture(&root, &selection, |_| out.clone())?;
            Ok(true)
        }
        Action::Runtime => {
            let checked = render::runtime::check(&root).map_err(|e| e.to_string())?;
            println!(
                "runtime {} ({}), canvas published {}",
                checked.version, checked.sha256, checked.live_published_at
            );
            Ok(true)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().action) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("bae-shots: {message}");
            ExitCode::FAILURE
        }
    }
}
