use bae_automation::{Automation, AutomationMetadataSource, AutomationTool};
use bae_core::app::{bootstrap, bootstrap_library_path, BootstrapError, RunningApp};
use bae_core::config::{init_keyring, Config};
use bae_core::import::MetadataSource;
use bae_core::keys::{BaeKeyServiceExt, KeyService};
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
    let token = key_service_token(&config.library_id)?;
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
        .map_err(CliError::Unavailable)?;
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
            bootstrap_library_path(path.clone(), 1000).map_err(bootstrap_error)
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
    bootstrap(id, 1000, None).map_err(bootstrap_error)
}

fn require_unlocked_for_headless(config: &Config) -> Result<(), CliError> {
    if !config.encryption_key_stored {
        return Ok(());
    }
    let key_service = KeyService::new(config.library_id.clone());
    match key_service.get_encryption_key() {
        Ok(Some(_)) => Ok(()),
        Ok(None) => Err(CliError::Unavailable(format!(
            "library '{}' is locked; unlock it before running headless automation",
            config.library_name
        ))),
        Err(e) => Err(CliError::Unavailable(format!(
            "failed to read encryption key for '{}': {e}",
            config.library_name
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
        .map(|config| config.library_id.clone())
        .map_err(|e| CliError::Config(e.to_string()))
}

fn key_service_token(library_id: &str) -> Result<String, CliError> {
    let service = KeyService::new(library_id.to_string());
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
