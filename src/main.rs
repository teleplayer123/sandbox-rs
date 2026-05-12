use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sandbox_rs::{
    error::{Result, SandboxError},
    http::{HttpClient, RequestOptions},
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

    /// Print the current sandbox config
    Config,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
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
            let sb = Sandbox::init(&dir)?;
            println!("Platform     : {}", plat.name());
            println!("Sandbox root : {}", sb.root.display());
            println!("Responses dir: {}", sb.config.response_dir_path(&sb.root).display());
            println!("Initialized.");
        }

        Cmd::Get { url, headers, query, save, print } => {
            let sb = Sandbox::init(&dir)?;
            let client = HttpClient::new(&sb.config)?;
            let resp = client.get(build_opts(&url, headers, query)?, &sb.config)?;
            print_status(resp.status);
            handle_output(&sb, resp, save, print)?;
        }

        Cmd::Post { url, json, form, headers, query, save, print } => {
            let sb = Sandbox::init(&dir)?;
            let client = HttpClient::new(&sb.config)?;
            let opts = build_opts(&url, headers, query)?;

            let resp = if let Some(raw) = json {
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
            let raw = toml::to_string_pretty(&sb.config).map_err(SandboxError::TomlSer)?;
            println!("{raw}");
        }
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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
