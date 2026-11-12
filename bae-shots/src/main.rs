//! bae-shots: takes the shots of the apps and checks them against the design
//! boards.

mod capture;
mod declared;
mod lint;
mod names;
mod render;

use clap::{Args, Parser, Subcommand};
use names::{Name, Platform, Prefix};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    /// Take the shots into shots/<platform>/.
    Run(Selection),
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
    let names: Vec<Name> = declared.into_iter().map(|d| d.name).collect();
    for platform in selection.platforms() {
        let out = std::path::absolute(out_for(platform)).map_err(|e| e.to_string())?;
        match platform {
            Platform::Mac => {
                capture::mac::capture(root, &names, &selection.prefixes, selection.windows, &out)
            }
            Platform::Android => Err(capture::CaptureError::NoCapture(platform)),
        }
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run(action: Action) -> Result<bool, String> {
    let root = repository_root()?;
    match action {
        Action::Lint => {
            let declared = declared::read(&root).map_err(|e| e.to_string())?;
            let problems = lint::problems(&root, &declared);
            for problem in &problems {
                eprintln!("{problem}");
            }
            Ok(problems.is_empty())
        }
        Action::Run(selection) => {
            capture(&root, &selection, |p| root.join("shots").join(p.word()))?;
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
