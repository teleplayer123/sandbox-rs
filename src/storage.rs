use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::{error::Result, http::HttpResponse, sandbox::Sandbox};

#[derive(Debug, Serialize, Deserialize)]
pub struct SavedResponse {
    pub timestamp: String,
    pub url: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl SavedResponse {
    pub fn from_http(resp: &HttpResponse) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            url: resp.url.clone(),
            status: resp.status,
            headers: resp.headers.clone(),
            body: resp.body.clone(),
        }
    }
}

/// Save an HTTP response as a JSON envelope in the sandbox responses directory.
/// Returns the path written.
pub fn save_response(sandbox: &Sandbox, filename: Option<&str>, resp: &HttpResponse) -> Result<PathBuf> {
    let name = match filename {
        Some(f) => f.to_string(),
        None => format!("{}.json", Utc::now().format("%Y%m%dT%H%M%S%3fZ")),
    };

    let dest = sandbox.response_path(&name)?;

    // Create any subdirectories inside the response dir that the caller named.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let envelope = SavedResponse::from_http(resp);
    let json = serde_json::to_string_pretty(&envelope)?;
    std::fs::write(&dest, json)?;
    log::info!("response saved → {}", dest.display());

    Ok(dest)
}

/// List all files stored in the sandbox responses directory.
pub fn list_responses(sandbox: &Sandbox) -> Result<Vec<PathBuf>> {
    let dir = sandbox.config.response_dir_path(&sandbox.root);
    let mut entries = vec![];

    for entry in std::fs::read_dir(dir)? {
        let e = entry?;
        if e.file_type()?.is_file() {
            entries.push(e.path());
        }
    }

    entries.sort();
    Ok(entries)
}

/// Read and deserialize a previously saved response by filename.
pub fn load_response(sandbox: &Sandbox, filename: &str) -> Result<SavedResponse> {
    let path = sandbox.response_path(filename)?;
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw)?)
}
