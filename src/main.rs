use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sandbox_rs::{
    config::LogConfig,
    error::{Result, SandboxError},
    http::{HttpClient, RequestOptions},
    logging,
    platform,
    sandbox::Sandbox,
    storage,
};

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "sandbox",
    version,
    about = "Secure sandboxed HTTP request runner — macOS, Windows, Linux"
)]
struct Cli {
    /// Sandbox root directory (defaults to the OS data dir for this platform)
    #[arg(short, long, value_name = "DIR")]
    dir: Option<PathBuf>,

    /// Allow reads from this directory even though it is outside the sandbox root.
    /// Repeatable. Use with `init` to persist; other commands apply for this session only.
    #[arg(short = 'R', long = "readonly", value_name = "DIR")]
    readonly: Vec<PathBuf>,

    /// Disable file logging for this session, overriding sandbox.toml.
    #[arg(long)]
    no_log: bool,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Initialize a sandbox directory and print its location
    Init,

    /// Send a GET request
    Get {
        url: String,
        /// Extra headers: "Name: Value" (repeatable)
        #[arg(short = 'H', long = "header", value_name = "HEADER")]
        headers: Vec<String>,
        /// Query params: "key=value" (repeatable)
        #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
        query: Vec<String>,
        /// Save response to this filename inside the responses dir
        #[arg(short, long, value_name = "FILENAME")]
        save: Option<String>,
        /// Print the response body to stdout
        #[arg(short, long)]
        print: bool,
    },

    /// Send a POST request
    Post {
        url: String,
        /// JSON body (raw string)
        #[arg(short, long, value_name = "JSON", group = "body")]
        json: Option<String>,
        /// Form fields: "key=value" — sends application/x-www-form-urlencoded
        #[arg(short, long, value_name = "KEY=VALUE", group = "body")]
        form: Vec<String>,
        /// Read the request body from a file (may be inside a readonly dir)
        #[arg(long, value_name = "PATH", group = "body")]
        body_file: Option<PathBuf>,
        /// Extra headers: "Name: Value" (repeatable)
        #[arg(short = 'H', long = "header", value_name = "HEADER")]
        headers: Vec<String>,
        /// Query params: "key=value" (repeatable)
        #[arg(short = 'q', long = "query", value_name = "KEY=VALUE")]
        query: Vec<String>,
        /// Save response to this filename inside the responses dir
        #[arg(short, long, value_name = "FILENAME")]
        save: Option<String>,
        /// Print the response body to stdout
        #[arg(short, long)]
        print: bool,
    },

    /// List saved responses
    List,

    /// Print a saved response by filename
    Show { filename: String },

    /// Print the current sandbox config (includes readonly_dirs)
    Config,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
        log::error!("{e}");
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let plat = platform::current();
    let dir = resolve_dir(cli.dir, &*plat);

    match cli.command {
        Cmd::Init => {
            let mut sb = Sandbox::init(&dir)?;
            if !cli.readonly.is_empty() {
                sb.add_readonly_dirs(&cli.readonly);
                sb.config.save(&sb.root)?;
                println!("Readonly dirs saved to config.");
            }
            if cli.no_log {
                sb.config.log.enabled = false;
                sb.config.save(&sb.root)?;
                println!("Logging disabled and saved to config.");
            }
            start_logging(&sb, false)?; // respect whatever is now in config
            log::info!("sandbox initialized (platform={})", plat.name());
            println!("Platform     : {}", plat.name());
            println!("Sandbox root : {}", sb.root.display());
            println!("Responses dir: {}", sb.config.response_dir_path(&sb.root).display());
            println!("Log file     : {}", if sb.config.log.enabled {
                sb.root.join(&sb.config.log.file).display().to_string()
            } else {
                "disabled".to_string()
            });
            if !sb.config.readonly_dirs.is_empty() {
                for d in &sb.config.readonly_dirs {
                    println!("Readonly dir : {}", d.display());
                }
            }
            println!("Initialized.");
        }

        Cmd::Get { url, headers, query, save, print } => {
            let mut sb = Sandbox::init(&dir)?;
            sb.add_readonly_dirs(&cli.readonly);
            start_logging(&sb, cli.no_log)?;
            let client = HttpClient::new(&sb.config)?;
            let resp = client.get(build_opts(&url, headers, query)?, &sb.config)?;
            print_status(resp.status);
            handle_output(&sb, resp, save, print)?;
        }

        Cmd::Post { url, json, form, body_file, headers, query, save, print } => {
            let mut sb = Sandbox::init(&dir)?;
            sb.add_readonly_dirs(&cli.readonly);
            start_logging(&sb, cli.no_log)?;
            let client = HttpClient::new(&sb.config)?;
            let opts = build_opts(&url, headers, query)?;

            let resp = if let Some(raw) = json {
                let body: Value = serde_json::from_str(&raw).map_err(SandboxError::Json)?;
                client.post_json(opts, body, &sb.config)?
            } else if let Some(file_path) = body_file {
                let path_str = file_path.to_string_lossy().into_owned();
                let resolved = sb.safe_read_path(&path_str)?;
                let raw = std::fs::read_to_string(&resolved)?;
                let body: Value = serde_json::from_str(&raw).map_err(SandboxError::Json)?;
                client.post_json(opts, body, &sb.config)?
            } else if !form.is_empty() {
                client.post_form(opts, parse_kv(&form)?, &sb.config)?
            } else {
                client.post_json(opts, Value::Null, &sb.config)?
            };

            print_status(resp.status);
            handle_output(&sb, resp, save, print)?;
        }

        Cmd::List => {
            let sb = Sandbox::init(&dir)?;
            start_logging(&sb, cli.no_log)?;
            let files = storage::list_responses(&sb)?;
            if files.is_empty() {
                println!("No saved responses.");
            } else {
                for f in &files {
                    println!("{}", f.display());
                }
            }
        }

        Cmd::Show { filename } => {
            let sb = Sandbox::init(&dir)?;
            start_logging(&sb, cli.no_log)?;
            let saved = storage::load_response(&sb, &filename)?;
            println!("URL    : {}", saved.url);
            println!("Status : {}", saved.status);
            println!("Time   : {}", saved.timestamp);
            println!("---");
            let body = serde_json::from_str::<Value>(&saved.body)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or(saved.body);
            println!("{body}");
        }

        Cmd::Config => {
            let sb = Sandbox::init(&dir)?;
            start_logging(&sb, cli.no_log)?;
            let raw = toml::to_string_pretty(&sb.config).map_err(SandboxError::TomlSer)?;
            println!("{raw}");
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Start the file logger, respecting the `--no-log` override.
fn start_logging(sb: &Sandbox, no_log: bool) -> Result<()> {
    let effective = LogConfig {
        enabled: sb.config.log.enabled && !no_log,
        ..sb.config.log.clone()
    };
    logging::init(&effective, &sb.root)
}

fn resolve_dir(explicit: Option<PathBuf>, plat: &dyn platform::Platform) -> PathBuf {
    explicit
        .or_else(|| plat.default_sandbox_dir())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn build_opts(
    url: &str,
    headers: Vec<String>,
    query: Vec<String>,
) -> Result<RequestOptions<'_>> {
    Ok(RequestOptions {
        url,
        headers: parse_headers(&headers)?,
        query: parse_kv(&query)?,
    })
}

fn parse_headers(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|s| {
            s.split_once(':')
                .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                .ok_or_else(|| SandboxError::InvalidArg(
                    format!("invalid header '{s}' (expected 'Name: Value')")
                ))
        })
        .collect()
}

fn parse_kv(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|s| {
            s.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .ok_or_else(|| SandboxError::InvalidArg(
                    format!("invalid key=value pair '{s}'")
                ))
        })
        .collect()
}

fn print_status(status: u16) {
    println!("HTTP {status}");
}

fn handle_output(
    sb: &Sandbox,
    resp: sandbox_rs::http::HttpResponse,
    save: Option<String>,
    print: bool,
) -> Result<()> {
    if print {
        let body = resp.pretty_json().unwrap_or_else(|| resp.body.clone());
        println!("{body}");
    }
    match save {
        Some(name) => {
            let path = storage::save_response(sb, Some(&name), &resp)?;
            println!("Saved to: {}", path.display());
        }
        None if !print => {
            let path = storage::save_response(sb, None, &resp)?;
            println!("Auto-saved to: {}", path.display());
        }
        _ => {}
    }
    Ok(())
}
