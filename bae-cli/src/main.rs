#![deny(unreachable_pub, dead_code)]

use bae_automation::{Automation, AutomationMetadataSource, AutomationTool};
use bae_core::app::{bootstrap, bootstrap_library_path, BootstrapError, RunningApp};
use bae_core::config::{init_keyring, Config};
use bae_core::import::MetadataSource;
use bae_core::keys::{BaeStoreKeysExt, StoreKeys};
use bae_mcp::{proxy_stdio, serve_stdio, McpClient};
use clap::{Parser, Subcommand};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

fn main() {
    let cli = Cli::parse();
    let pretty = cli.pretty;
    match run(cli) {
        Ok(CliOutput::Json(value)) => print_json_stdout(&value, pretty),
        Ok(CliOutput::Silent) => {}
        Err(error) => {
            print_json_stderr(&error.body(), pretty);
            std::process::exit(1);
        }
    }
}

enum CliOutput {
    Json(Value),
    Silent,
}

fn stdio_completed_output() -> CliOutput {
    CliOutput::Silent
}

#[derive(Parser)]
#[command(name = "bae")]
struct Cli {
    #[arg(long)]
    pretty: bool,
    #[arg(long)]
    library_id: Option<String>,
    #[arg(long)]
    library_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    WatchedFolders {
        #[command(subcommand)]
        command: WatchedFoldersCommand,
    },
    Import {
        #[command(subcommand)]
        command: ImportCommand,
    },
    Release {
        #[command(subcommand)]
        command: ReleaseCommand,
    },
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Subcommand)]
enum ConfigCommand {
    Get,
}

#[derive(Subcommand)]
enum WatchedFoldersCommand {
    List,
    Add {
        path: String,
    },
    Remove {
        path: String,
    },
    Scan {
        #[arg(long)]
        wait: bool,
        #[arg(long, default_value_t = 60_000)]
        timeout_ms: u64,
    },
}

#[derive(Subcommand)]
enum ImportCommand {
    Candidates {
        #[command(subcommand)]
        command: ImportCandidatesCommand,
    },
    Candidate {
        #[command(subcommand)]
        command: ImportCandidateCommand,
    },
    Search {
        #[command(subcommand)]
        command: ImportSearchCommand,
    },
    Prefetch {
        source: MetadataSource,
        release_id: String,
    },
    PreviewFileTags {
        folder: String,
    },
    ShapeEdit {
        #[arg(long)]
        detail_json: String,
        #[arg(long)]
        choice_json: String,
    },
    Start {
        #[arg(long)]
        json: String,
    },
}

#[derive(Subcommand)]
enum ImportCandidatesCommand {
    List,
}

#[derive(Subcommand)]
enum ImportCandidateCommand {
    Get {
        candidate_key: String,
    },
    Skip {
        #[command(subcommand)]
        command: ImportCandidateSkipCommand,
    },
}

#[derive(Subcommand)]
enum ImportCandidateSkipCommand {
    Set {
        candidate_key: String,
        skipped: bool,
    },
}

#[derive(Subcommand)]
enum ImportSearchCommand {
    General {
        #[arg(long)]
        artist: String,
        #[arg(long)]
        album: String,
        #[arg(long)]
        source: MetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        #[arg(long)]
        source: MetadataSource,
    },
    Barcode {
        barcode: String,
        #[arg(long)]
        source: MetadataSource,
    },
}

#[derive(Subcommand)]
enum ReleaseCommand {
    Detail {
        release_id: String,
    },
    Export {
        #[arg(long)]
        release_id: String,
        #[arg(long)]
        target_dir: PathBuf,
    },
    ExportStatus,
    Reidentify {
        release_id: String,
        #[arg(long)]
        choice_json: String,
    },
    Metadata {
        #[command(subcommand)]
        command: ReleaseMetadataCommand,
    },
}

#[derive(Subcommand)]
enum ReleaseMetadataCommand {
    Reset {
        release_id: String,
    },
    Update {
        release_id: String,
        #[arg(long)]
        json: String,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    Search { query: String },
}

#[derive(Subcommand)]
enum McpCommand {
    Status,
    Token {
        #[command(subcommand)]
        command: Option<McpTokenCommand>,
    },
    Stdio {
        #[arg(long)]
        headless: bool,
    },
}

#[derive(Subcommand)]
enum McpTokenCommand {
    Generate,
    Set { token: String },
}

fn run(cli: Cli) -> Result<CliOutput, CliError> {
    init_keyring();
    let selector = LibrarySelector::from_options(cli.library_id, cli.library_path)?;

    if let Command::Mcp {
        command: McpCommand::Stdio { headless },
    } = &cli.command
    {
        if *headless || selector.is_headless() {
            return run_headless_stdio(selector);
        }
        return run_server_stdio();
    }

    match &cli.command {
        Command::Mcp {
            command: McpCommand::Status,
        } => return mcp_status(&selector).map(CliOutput::Json),
        Command::Mcp {
            command: McpCommand::Token { command: None },
        } => return mcp_token(&selector).map(CliOutput::Json),
        Command::Mcp {
            command:
                McpCommand::Token {
                    command: Some(McpTokenCommand::Generate),
                },
        } => return mcp_token_generate().map(CliOutput::Json),
        Command::Mcp {
            command:
                McpCommand::Token {
                    command: Some(McpTokenCommand::Set { token }),
                },
        } => return mcp_token_set(&selector, token.clone()).map(CliOutput::Json),
        _ => {}
    }

    let (tool, args) = tool_call_for_command(&cli.command)?;
    let value = if selector.is_headless() {
        run_headless_tool(selector, tool, args)?
    } else {
        run_server_tool(tool, args)?
    };
    Ok(CliOutput::Json(value))
}

#[derive(Clone)]
enum LibrarySelector {
    Active,
    Id(String),
    Path(PathBuf),
}

impl LibrarySelector {
    fn from_options(
        library_id: Option<String>,
        library_path: Option<PathBuf>,
    ) -> Result<Self, CliError> {
        match (library_id, library_path) {
            (Some(_), Some(_)) => Err(CliError::Validation(
                "--library-id and --library-path cannot be used together".to_string(),
            )),
            (Some(id), None) => Ok(Self::Id(id)),
            (None, Some(path)) => Ok(Self::Path(path)),
            (None, None) => Ok(Self::Active),
        }
    }

    fn is_headless(&self) -> bool {
        !matches!(self, Self::Active)
    }
}

fn tool_call_for_command(command: &Command) -> Result<(AutomationTool, Value), CliError> {
    match command {
        Command::Config {
            command: ConfigCommand::Get,
        } => Ok((AutomationTool::ConfigGet, Value::Null)),
        Command::WatchedFolders { command } => match command {
            WatchedFoldersCommand::List => Ok((AutomationTool::WatchedFoldersList, Value::Null)),
            WatchedFoldersCommand::Add { path } => {
                Ok((AutomationTool::WatchedFolderAdd, json!({ "path": path })))
            }
            WatchedFoldersCommand::Remove { path } => {
                Ok((AutomationTool::WatchedFolderRemove, json!({ "path": path })))
            }
            WatchedFoldersCommand::Scan { wait, timeout_ms } => {
                let wait = if *wait {
                    json!({ "mode": "until_finished", "timeout_ms": timeout_ms })
                } else {
                    json!({ "mode": "no_wait" })
                };
                Ok((AutomationTool::WatchedFoldersScan, wait))
            }
        },
        Command::Import { command } => match command {
            ImportCommand::Candidates {
                command: ImportCandidatesCommand::List,
            } => Ok((AutomationTool::ImportCandidatesList, Value::Null)),
            ImportCommand::Candidate { command } => match command {
                ImportCandidateCommand::Get { candidate_key } => Ok((
                    AutomationTool::ImportCandidateGet,
                    json!({ "candidate_key": candidate_key }),
                )),
                ImportCandidateCommand::Skip {
                    command:
                        ImportCandidateSkipCommand::Set {
                            candidate_key,
                            skipped,
                        },
                } => Ok((
                    AutomationTool::ImportCandidateSkipSet,
                    json!({ "candidate_key": candidate_key, "skipped": skipped }),
                )),
            },
            ImportCommand::Search { command } => {
                let query = match command {
                    ImportSearchCommand::General {
                        artist,
                        album,
                        source,
                    } => json!({
                        "kind": "general",
                        "artist": artist,
                        "album": album,
                        "source": AutomationMetadataSource::from(*source),
                    }),
                    ImportSearchCommand::CatalogNumber {
                        catalog_number,
                        source,
                    } => json!({
                        "kind": "catalog_number",
                        "catalog_number": catalog_number,
                        "source": AutomationMetadataSource::from(*source),
                    }),
                    ImportSearchCommand::Barcode { barcode, source } => json!({
                        "kind": "barcode",
                        "barcode": barcode,
                        "source": AutomationMetadataSource::from(*source),
                    }),
                };
                Ok((AutomationTool::ImportSearch, query))
            }
            ImportCommand::Prefetch { source, release_id } => Ok((
                AutomationTool::ImportReleasePrefetch,
                json!({ "source": AutomationMetadataSource::from(*source), "release_id": release_id }),
            )),
            ImportCommand::PreviewFileTags { folder } => Ok((
                AutomationTool::ImportFileTagsPreview,
                json!({ "folder": folder }),
            )),
            ImportCommand::ShapeEdit {
                detail_json,
                choice_json,
            } => Ok((
                AutomationTool::ImportReleaseEditShape,
                json!({
                    "detail": read_json_value(detail_json)?,
                    "choice": read_json_value(choice_json)?,
                }),
            )),
            ImportCommand::Start { json: path } => {
                Ok((AutomationTool::ImportStart, read_json_value(path)?))
            }
        },
        Command::Release { command } => match command {
            ReleaseCommand::Detail { release_id } => Ok((
                AutomationTool::ReleaseDetailGet,
                json!({ "release_id": release_id }),
            )),
            ReleaseCommand::Export {
                release_id,
                target_dir,
            } => {
                // clap hands us the raw OS path, which on Unix may not be UTF-8. The
                // export target must round-trip as a string, so reject a non-UTF-8
                // path loudly instead of lossily rewriting it to a different dir.
                let target_dir = target_dir.to_str().ok_or_else(|| {
                    CliError::Validation(format!(
                        "target directory is not valid UTF-8: {}",
                        target_dir.display()
                    ))
                })?;
                Ok((
                    AutomationTool::ReleaseExport,
                    json!({
                        "release_id": release_id,
                        "target_dir": target_dir,
                    }),
                ))
            }
            ReleaseCommand::ExportStatus => Ok((AutomationTool::ExportStatus, json!({}))),
            ReleaseCommand::Reidentify {
                release_id,
                choice_json,
            } => Ok((
                AutomationTool::ReleaseReidentify,
                json!({ "release_id": release_id, "choice": read_json_value(choice_json)? }),
            )),
            ReleaseCommand::Metadata { command } => match command {
                ReleaseMetadataCommand::Reset { release_id } => Ok((
                    AutomationTool::ReleaseMetadataReset,
                    json!({ "release_id": release_id }),
                )),
                ReleaseMetadataCommand::Update {
                    release_id,
                    json: path,
                } => Ok((
                    AutomationTool::ReleaseMetadataUpdate,
                    json!({ "release_id": release_id, "edit": read_json_value(path)? }),
                )),
            },
        },
        Command::Library {
            command: LibraryCommand::Search { query },
        } => Ok((AutomationTool::LibrarySearch, json!({ "query": query }))),
        Command::Mcp { .. } => Err(CliError::Validation(
            "MCP command does not map to an automation tool".to_string(),
        )),
    }
}

fn run_server_tool(tool: AutomationTool, args: Value) -> Result<Value, CliError> {
    let endpoint = active_mcp_endpoint()?;
    endpoint
        .runtime
        .block_on(McpClient::new(endpoint.uri, endpoint.token).call_tool(tool.name(), args))
        .map_err(|e| CliError::Unavailable(e.to_string()))
}

fn run_headless_tool(
    selector: LibrarySelector,
    tool: AutomationTool,
    args: Value,
) -> Result<Value, CliError> {
    let app = bootstrap_for_selector(&selector)?;
    let automation = Automation::new(app.services.clone(), app.runtime.handle().clone());
    automation.start_event_indexing();
    let value = app.runtime.block_on(automation.call_tool(tool, args))?;
    drop(app);
    Ok(value)
}

fn run_headless_stdio(selector: LibrarySelector) -> Result<CliOutput, CliError> {
    let app = bootstrap_for_selector(&selector)?;
    let automation = Automation::new(app.services.clone(), app.runtime.handle().clone());
    app.runtime
        .block_on(serve_stdio(automation))
        .map_err(|e| CliError::Unavailable(e.to_string()))?;
    Ok(stdio_completed_output())
}

fn run_server_stdio() -> Result<CliOutput, CliError> {
    let endpoint = active_mcp_endpoint()?;
    endpoint
        .runtime
        .block_on(proxy_stdio(endpoint.uri, endpoint.token))
        .map_err(|e| CliError::Unavailable(e.to_string()))?;
    Ok(stdio_completed_output())
}

fn mcp_status(selector: &LibrarySelector) -> Result<Value, CliError> {
    if selector.is_headless() {
        let app = bootstrap_for_selector(selector)?;
        let automation = Automation::new(app.services.clone(), app.runtime.handle().clone());
        let status = automation.status();
        drop(app);
        return Ok(serde_json::to_value(status)?);
    }

    let endpoint = active_mcp_endpoint()?;
    let uri = endpoint.uri.clone();
    match endpoint
        .runtime
        .block_on(McpClient::new(endpoint.uri, endpoint.token).call_tool("config_get", Value::Null))
    {
        Ok(config_value) => Ok(json!({ "status": "running", "url": uri, "config": config_value })),
        Err(e) => Ok(json!({ "status": "unavailable", "url": uri, "error": e.to_string() })),
    }
}

struct ActiveMcpEndpoint {
    runtime: tokio::runtime::Runtime,
    uri: String,
    token: String,
}

fn active_mcp_endpoint() -> Result<ActiveMcpEndpoint, CliError> {
    let config = active_config()?;
    let token = key_service_token(&config.store_id)?;
    let uri = format!("http://127.0.0.1:{}/mcp", config.mcp.port);
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|e| CliError::Internal(format!("failed to create runtime: {e}")))?;
    Ok(ActiveMcpEndpoint {
        runtime,
        uri,
        token,
    })
}

fn mcp_token(selector: &LibrarySelector) -> Result<Value, CliError> {
    let library_id = resolve_library_id(selector)?;
    let token = key_service_token(&library_id)?;
    Ok(json!({ "token": token }))
}

fn mcp_token_generate() -> Result<Value, CliError> {
    Ok(json!({ "token": bae_core::library::generate_mcp_token() }))
}

fn mcp_token_set(selector: &LibrarySelector, token: String) -> Result<Value, CliError> {
    let app = bootstrap_for_selector(selector)?;
    app.services
        .library_manager()
        .set_mcp_token(token.clone())
        .map_err(|e| CliError::Unavailable(e.to_string()))?;
    drop(app);
    Ok(json!({ "token": token }))
}

fn active_config() -> Result<Config, CliError> {
    let id = resolve_library_id(&LibrarySelector::Active)?;
    Config::load_registered_library(&id, &coven::UuidProvider)
        .map_err(|e| CliError::Config(e.to_string()))
}

fn bootstrap_for_selector(selector: &LibrarySelector) -> Result<RunningApp, CliError> {
    match selector {
        LibrarySelector::Id(id) => bootstrap_registered_library(id.clone()),
        LibrarySelector::Path(path) => {
            let config = Config::load_from_library_path(path.clone(), &coven::UuidProvider)
                .map_err(|e| CliError::Config(e.to_string()))?;
            require_unlocked_for_headless(&config)?;
            bootstrap_library_path(path.clone(), 1000, false).map_err(bootstrap_error)
        }
        LibrarySelector::Active => {
            let id = resolve_library_id(selector)?;
            bootstrap_registered_library(id)
        }
    }
}

fn bootstrap_registered_library(id: String) -> Result<RunningApp, CliError> {
    let config = Config::load_registered_library(&id, &coven::UuidProvider)
        .map_err(|e| CliError::Config(e.to_string()))?;
    require_unlocked_for_headless(&config)?;
    bootstrap(id, 1000, false, None).map_err(bootstrap_error)
}

fn require_unlocked_for_headless(config: &Config) -> Result<(), CliError> {
    if !config.encryption_key_stored {
        return Ok(());
    }
    let key_service = StoreKeys::new(config.store_id.clone());
    match key_service.get_encryption_key() {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(CliError::Unavailable(format!(
            "library '{}' is locked; unlock it before running headless automation",
            config.store_name
        ))),
        Err(e) => Err(CliError::Unavailable(format!(
            "failed to read encryption key for '{}': {e}",
            config.store_name
        ))),
    }
}

fn resolve_library_id(selector: &LibrarySelector) -> Result<String, CliError> {
    match selector {
        LibrarySelector::Id(id) => Ok(id.clone()),
        LibrarySelector::Path(path) => library_id_from_path(path),
        LibrarySelector::Active => Config::active_library_id()
            .map_err(|e| CliError::Config(e.to_string()))?
            .ok_or_else(|| {
                CliError::Unavailable(
                    "no active library; pass --library-id or --library-path".to_string(),
                )
            }),
    }
}

fn library_id_from_path(path: &Path) -> Result<String, CliError> {
    Config::load_from_library_path(path.to_path_buf(), &coven::UuidProvider)
        .map(|config| config.store_id.clone())
        .map_err(|e| CliError::Config(e.to_string()))
}

fn key_service_token(library_id: &str) -> Result<String, CliError> {
    let service = StoreKeys::new(library_id.to_string());
    service
        .get_mcp_token()
        .map_err(|e| CliError::Unavailable(e.to_string()))?
        .ok_or_else(|| CliError::Unavailable("MCP token is not stored in the keyring".to_string()))
}

fn read_json_value(path: &str) -> Result<Value, CliError> {
    let text = if path == "-" {
        std::io::read_to_string(std::io::stdin())
            .map_err(|e| CliError::Config(format!("failed to read stdin: {e}")))?
    } else {
        fs::read_to_string(path)
            .map_err(|e| CliError::Config(format!("failed to read {path}: {e}")))?
    };
    serde_json::from_str(&text).map_err(CliError::from)
}

fn bootstrap_error(error: BootstrapError) -> CliError {
    match error {
        BootstrapError::LibraryNotFound(id) => {
            CliError::Unavailable(format!("library not found: {id}"))
        }
        BootstrapError::Config(e) => CliError::Config(e),
        BootstrapError::Database(e) => CliError::Unavailable(e),
        BootstrapError::Internal(e) => CliError::Internal(e),
    }
}

fn print_json_stdout(value: &Value, pretty: bool) {
    println!("{}", json_output(value, pretty, "serialize JSON output"));
}

fn print_json_stderr(value: &Value, pretty: bool) {
    eprintln!("{}", json_output(value, pretty, "serialize JSON error"));
}

fn json_output(value: &Value, pretty: bool, expectation: &str) -> String {
    if pretty {
        serde_json::to_string_pretty(value).expect(expectation)
    } else {
        serde_json::to_string(value).expect(expectation)
    }
}

#[derive(Debug, Error)]
enum CliError {
    #[error("config: {0}")]
    Config(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("internal: {0}")]
    Internal(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("automation: {0}")]
    Automation(#[from] bae_automation::AutomationError),
}

impl CliError {
    fn body(&self) -> Value {
        let kind = match self {
            CliError::Config(_) => "config",
            CliError::Validation(_) => "validation",
            CliError::Unavailable(_) => "unavailable",
            CliError::Internal(_) => "internal",
            CliError::Json(_) => "validation",
            CliError::Automation(error) => error.kind(),
        };
        json!({
            "error": {
                "kind": kind,
                "message": self.to_string(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn call(command: Command) -> (AutomationTool, Value) {
        tool_call_for_command(&command).expect("command maps to a tool call")
    }

    /// The commands whose argument object is built inline from clap-parsed
    /// values (no file reads, no path validation). Each row is the command and
    /// the exact `(tool, args)` it must produce.
    #[test]
    fn inline_commands_map_to_tool_and_args() {
        let cases: Vec<(Command, AutomationTool, Value)> = vec![
            (
                Command::Config {
                    command: ConfigCommand::Get,
                },
                AutomationTool::ConfigGet,
                Value::Null,
            ),
            (
                Command::WatchedFolders {
                    command: WatchedFoldersCommand::List,
                },
                AutomationTool::WatchedFoldersList,
                Value::Null,
            ),
            (
                Command::WatchedFolders {
                    command: WatchedFoldersCommand::Add {
                        path: "/music/inbox".to_string(),
                    },
                },
                AutomationTool::WatchedFolderAdd,
                json!({ "path": "/music/inbox" }),
            ),
            (
                Command::WatchedFolders {
                    command: WatchedFoldersCommand::Remove {
                        path: "/music/inbox".to_string(),
                    },
                },
                AutomationTool::WatchedFolderRemove,
                json!({ "path": "/music/inbox" }),
            ),
            (
                Command::WatchedFolders {
                    command: WatchedFoldersCommand::Scan {
                        wait: false,
                        timeout_ms: 60_000,
                    },
                },
                AutomationTool::WatchedFoldersScan,
                json!({ "mode": "no_wait" }),
            ),
            (
                Command::WatchedFolders {
                    command: WatchedFoldersCommand::Scan {
                        wait: true,
                        timeout_ms: 45_000,
                    },
                },
                AutomationTool::WatchedFoldersScan,
                json!({ "mode": "until_finished", "timeout_ms": 45_000 }),
            ),
            (
                Command::Import {
                    command: ImportCommand::Candidates {
                        command: ImportCandidatesCommand::List,
                    },
                },
                AutomationTool::ImportCandidatesList,
                Value::Null,
            ),
            (
                Command::Import {
                    command: ImportCommand::Candidate {
                        command: ImportCandidateCommand::Get {
                            candidate_key: "cand-1".to_string(),
                        },
                    },
                },
                AutomationTool::ImportCandidateGet,
                json!({ "candidate_key": "cand-1" }),
            ),
            (
                Command::Import {
                    command: ImportCommand::Candidate {
                        command: ImportCandidateCommand::Skip {
                            command: ImportCandidateSkipCommand::Set {
                                candidate_key: "cand-1".to_string(),
                                skipped: true,
                            },
                        },
                    },
                },
                AutomationTool::ImportCandidateSkipSet,
                json!({ "candidate_key": "cand-1", "skipped": true }),
            ),
            (
                Command::Import {
                    command: ImportCommand::Prefetch {
                        source: MetadataSource::MusicBrainz,
                        release_id: "rel-123".to_string(),
                    },
                },
                AutomationTool::ImportReleasePrefetch,
                json!({ "source": "music_brainz", "release_id": "rel-123" }),
            ),
            (
                Command::Import {
                    command: ImportCommand::PreviewFileTags {
                        folder: "/music/inbox".to_string(),
                    },
                },
                AutomationTool::ImportFileTagsPreview,
                json!({ "folder": "/music/inbox" }),
            ),
            (
                Command::Release {
                    command: ReleaseCommand::Detail {
                        release_id: "rel-123".to_string(),
                    },
                },
                AutomationTool::ReleaseDetailGet,
                json!({ "release_id": "rel-123" }),
            ),
            (
                Command::Release {
                    command: ReleaseCommand::ExportStatus,
                },
                AutomationTool::ExportStatus,
                json!({}),
            ),
            (
                Command::Release {
                    command: ReleaseCommand::Metadata {
                        command: ReleaseMetadataCommand::Reset {
                            release_id: "rel-123".to_string(),
                        },
                    },
                },
                AutomationTool::ReleaseMetadataReset,
                json!({ "release_id": "rel-123" }),
            ),
            (
                Command::Library {
                    command: LibraryCommand::Search {
                        query: "artist album".to_string(),
                    },
                },
                AutomationTool::LibrarySearch,
                json!({ "query": "artist album" }),
            ),
        ];

        for (command, expected_tool, expected_args) in cases {
            let (tool, args) = call(command);
            assert_eq!(tool, expected_tool);
            assert_eq!(args, expected_args);
        }
    }

    /// The three import-search variants each carry a `kind` discriminator plus
    /// the fields specific to that kind; the source enum renders snake_case.
    #[test]
    fn import_search_kinds_produce_discriminated_queries() {
        let general = call(Command::Import {
            command: ImportCommand::Search {
                command: ImportSearchCommand::General {
                    artist: "Artist Name".to_string(),
                    album: "Album Title".to_string(),
                    source: MetadataSource::MusicBrainz,
                },
            },
        });
        assert_eq!(general.0, AutomationTool::ImportSearch);
        assert_eq!(
            general.1,
            json!({
                "kind": "general",
                "artist": "Artist Name",
                "album": "Album Title",
                "source": "music_brainz",
            })
        );

        let catalog = call(Command::Import {
            command: ImportCommand::Search {
                command: ImportSearchCommand::CatalogNumber {
                    catalog_number: "CAT-001".to_string(),
                    source: MetadataSource::Discogs,
                },
            },
        });
        assert_eq!(catalog.0, AutomationTool::ImportSearch);
        assert_eq!(
            catalog.1,
            json!({
                "kind": "catalog_number",
                "catalog_number": "CAT-001",
                "source": "discogs",
            })
        );

        let barcode = call(Command::Import {
            command: ImportCommand::Search {
                command: ImportSearchCommand::Barcode {
                    barcode: "0123456789".to_string(),
                    source: MetadataSource::Discogs,
                },
            },
        });
        assert_eq!(barcode.0, AutomationTool::ImportSearch);
        assert_eq!(
            barcode.1,
            json!({
                "kind": "barcode",
                "barcode": "0123456789",
                "source": "discogs",
            })
        );
    }

    #[test]
    fn export_passes_utf8_target_dir_as_string() {
        let (tool, args) = call(Command::Release {
            command: ReleaseCommand::Export {
                release_id: "rel-123".to_string(),
                target_dir: PathBuf::from("/export/here"),
            },
        });
        assert_eq!(tool, AutomationTool::ReleaseExport);
        assert_eq!(
            args,
            json!({ "release_id": "rel-123", "target_dir": "/export/here" })
        );
    }

    /// clap hands the export target as a raw OS path, which on Unix may hold
    /// bytes that aren't valid UTF-8. Such a path can't round-trip as a JSON
    /// string, so the mapping must reject it loudly rather than lossily rewrite
    /// it to a different directory.
    #[cfg(unix)]
    #[test]
    fn export_rejects_non_utf8_target_dir() {
        use std::os::unix::ffi::OsStringExt;
        let non_utf8 = std::ffi::OsString::from_vec(vec![0x2f, 0x66, 0x6f, 0x80, 0x6f]);
        let command = Command::Release {
            command: ReleaseCommand::Export {
                release_id: "rel-123".to_string(),
                target_dir: PathBuf::from(non_utf8),
            },
        };
        let error = tool_call_for_command(&command).expect_err("non-UTF-8 target is rejected");
        assert!(matches!(error, CliError::Validation(_)));
        assert!(error.to_string().contains("not valid UTF-8"));
    }

    #[test]
    fn mcp_commands_do_not_map_to_a_tool() {
        let error = tool_call_for_command(&Command::Mcp {
            command: McpCommand::Status,
        })
        .expect_err("MCP has no automation tool");
        assert!(matches!(error, CliError::Validation(_)));
    }

    static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

    fn write_temp_json(contents: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        path.push(format!("bae-cli-test-{}-{seq}.json", std::process::id()));
        fs::write(&path, contents).expect("write temp json fixture");
        path
    }

    /// The file-backed commands read their JSON payload from the path clap
    /// parsed and embed the parsed value in the tool args.
    #[test]
    fn file_backed_commands_read_and_embed_json() {
        let start_path = write_temp_json(r#"{"folder":"/music/inbox"}"#);
        let (tool, args) = call(Command::Import {
            command: ImportCommand::Start {
                json: start_path.to_str().unwrap().to_string(),
            },
        });
        assert_eq!(tool, AutomationTool::ImportStart);
        assert_eq!(args, json!({ "folder": "/music/inbox" }));
        fs::remove_file(&start_path).ok();

        let detail_path = write_temp_json(r#"{"album_title":"Album Title"}"#);
        let choice_path = write_temp_json(r#"{"kind":"unknown"}"#);
        let (tool, args) = call(Command::Import {
            command: ImportCommand::ShapeEdit {
                detail_json: detail_path.to_str().unwrap().to_string(),
                choice_json: choice_path.to_str().unwrap().to_string(),
            },
        });
        assert_eq!(tool, AutomationTool::ImportReleaseEditShape);
        assert_eq!(
            args,
            json!({
                "detail": { "album_title": "Album Title" },
                "choice": { "kind": "unknown" },
            })
        );
        fs::remove_file(&detail_path).ok();
        fs::remove_file(&choice_path).ok();

        let choice_path = write_temp_json(r#"{"kind":"exact","release_id":"rel-123"}"#);
        let (tool, args) = call(Command::Release {
            command: ReleaseCommand::Reidentify {
                release_id: "rel-123".to_string(),
                choice_json: choice_path.to_str().unwrap().to_string(),
            },
        });
        assert_eq!(tool, AutomationTool::ReleaseReidentify);
        assert_eq!(
            args,
            json!({
                "release_id": "rel-123",
                "choice": { "kind": "exact", "release_id": "rel-123" },
            })
        );
        fs::remove_file(&choice_path).ok();

        let edit_path = write_temp_json(r#"{"year":1999}"#);
        let (tool, args) = call(Command::Release {
            command: ReleaseCommand::Metadata {
                command: ReleaseMetadataCommand::Update {
                    release_id: "rel-123".to_string(),
                    json: edit_path.to_str().unwrap().to_string(),
                },
            },
        });
        assert_eq!(tool, AutomationTool::ReleaseMetadataUpdate);
        assert_eq!(
            args,
            json!({ "release_id": "rel-123", "edit": { "year": 1999 } })
        );
        fs::remove_file(&edit_path).ok();
    }

    #[test]
    fn file_backed_command_surfaces_missing_file() {
        let error = tool_call_for_command(&Command::Import {
            command: ImportCommand::Start {
                json: "/nonexistent/bae-cli-missing.json".to_string(),
            },
        })
        .expect_err("missing file fails the mapping");
        assert!(matches!(error, CliError::Config(_)));
    }

    #[test]
    fn selector_defaults_to_active_when_no_flags() {
        let selector = LibrarySelector::from_options(None, None).expect("no flags is valid");
        assert!(matches!(selector, LibrarySelector::Active));
        assert!(!selector.is_headless());
    }

    #[test]
    fn selector_id_flag_is_headless() {
        let selector = LibrarySelector::from_options(Some("lib-1".to_string()), None)
            .expect("library id alone is valid");
        assert!(matches!(selector, LibrarySelector::Id(ref id) if id == "lib-1"));
        assert!(selector.is_headless());
    }

    #[test]
    fn selector_path_flag_is_headless() {
        let selector = LibrarySelector::from_options(None, Some(PathBuf::from("/lib/here")))
            .expect("library path alone is valid");
        assert!(
            matches!(selector, LibrarySelector::Path(ref path) if path == &PathBuf::from("/lib/here"))
        );
        assert!(selector.is_headless());
    }

    #[test]
    fn selector_rejects_id_and_path_together() {
        let result =
            LibrarySelector::from_options(Some("lib-1".to_string()), Some(PathBuf::from("/lib")));
        match result {
            Err(CliError::Validation(message)) => {
                assert!(message.contains("cannot be used together"));
            }
            Ok(_) => panic!("expected a validation error, got a selector"),
            Err(other) => panic!("expected a validation error, got {other}"),
        }
    }
}
