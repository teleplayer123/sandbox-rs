# sandbox-rs

A cross-platform CLI and Rust library for making HTTP requests inside a secure, isolated directory environment. Every request can be saved to the sandbox automatically, and all file I/O is path-jailed so nothing can escape the sandbox root.

Runs on **macOS**, **Linux**, and **Windows**.

---

## Install

```sh
git clone <repo>
cd sandbox-rs
cargo build --release
# binary at: target/release/sandbox
```

---

## Quick start

```sh
# Initialise the sandbox (creates sandbox.toml, responses/, sandbox.log)
sandbox init

# GET a URL — response is auto-saved to the responses/ directory
sandbox get https://httpbin.org/get

# POST JSON — print the response and save it with a specific name
sandbox post https://api.osv.dev/v1/query \
  --json '{"package":{"name":"jinja2","ecosystem":"PyPI"}}' \
  --save jinja2.json \
  --print

# List all saved responses
sandbox list

# Print a saved response
sandbox show jinja2.json
```

---

## Sandbox directory

By default the sandbox root is the OS data directory for this platform:

| Platform | Default location |
|----------|-----------------|
| macOS    | `~/Library/Application Support/sandbox-rs/` |
| Linux    | `~/.local/share/sandbox-rs/` |
| Windows  | `%APPDATA%\sandbox-rs\` |

Use `--dir` to point at any directory instead:

```sh
sandbox --dir ./my-sandbox init
sandbox --dir ./my-sandbox get https://httpbin.org/get
```

---

## Commands

### `init`

Creates the sandbox directory structure and writes `sandbox.toml` with defaults.

```sh
sandbox init

# Use a custom directory and set a readonly exception (saved to sandbox.toml)
sandbox --dir ./work init --readonly /usr/share/data

# Disable logging permanently for this sandbox
sandbox init --no-log
```

### `get`

```sh
sandbox get <URL> [OPTIONS]

# With custom headers and query parameters
sandbox get https://httpbin.org/get \
  -H "Authorization: Bearer mytoken" \
  -q page=2 -q per_page=10

# Print to stdout without saving
sandbox get https://httpbin.org/get --print

# Save under a specific filename
sandbox get https://httpbin.org/get --save httpbin.json
```

### `post`

```sh
sandbox post <URL> [OPTIONS]

# JSON body
sandbox post https://httpbin.org/post \
  --json '{"key":"value"}' \
  --print

# Form-encoded body
sandbox post https://httpbin.org/post \
  --form username=alice --form password=secret

# Read body from a file (file may be inside a readonly dir)
sandbox post https://httpbin.org/post \
  --body-file ./payload.json

# With a custom header
sandbox post https://api.example.com/ingest \
  -H "X-API-Key: abc123" \
  --json '{"event":"deploy"}' \
  --save deploy-response.json
```

### `list` / `show`

```sh
# List all saved responses
sandbox list

# Print the body of a saved response
sandbox show jinja2.json
```

### `config`

Print the current `sandbox.toml` as TOML:

```sh
sandbox config
```

---

## Save behaviour

| Flags used | What happens |
|------------|-------------|
| neither `--print` nor `--save` | Auto-saved to `responses/<timestamp>.json` |
| `--save <name>` | Saved to `responses/<name>` |
| `--print` | Printed to stdout, **not** saved |
| `--print` + `--save <name>` | Printed **and** saved |

---

## Readonly directories

By default the sandbox only allows writing to — and reading from — its own root. Use `--readonly` to grant read access to directories outside the sandbox. Writes to those directories are always blocked.

```sh
# Allow reading from /etc for this session only
sandbox --readonly /etc get https://httpbin.org/get

# Persist the exception to sandbox.toml so every future command picks it up
sandbox init --readonly /usr/share/datasets
```

When posting, `--body-file` resolves the file through the readonly guard:

```sh
sandbox --readonly /data \
  post https://api.example.com/upload \
  --body-file /data/payload.json
```

---

## Logging

File logging is on by default. Logs are written to `sandbox.log` inside the sandbox root.

```sh
# Disable logging for one command
sandbox --no-log get https://httpbin.org/get

# Disable logging permanently (saved to sandbox.toml)
sandbox init --no-log
```

To change log level or filename, edit `sandbox.toml`:

```toml
[log]
enabled = true
file = "sandbox.log"
level = "debug"   # error | warn | info | debug | trace
```

---

## Configuration reference (`sandbox.toml`)

```toml
version = "0.1.0"
response_dir = "responses"   # where saved responses are written
timeout_secs = 30            # HTTP request timeout
default_headers = []         # applied to every request, e.g. [["Authorization", "Bearer x"]]
readonly_dirs = []           # absolute paths readable from outside the sandbox

[log]
enabled = true
file = "sandbox.log"
level = "info"
```

---

## Using as a library

Add to your `Cargo.toml`:

```toml
sandbox-rs = { path = "../sandbox-rs" }
```

```rust
use sandbox_rs::{Sandbox, logging};

let sandbox = Sandbox::init(std::path::Path::new("./my-sandbox"))?;
logging::init(&sandbox.config.log, &sandbox.root)?;

let client = sandbox_rs::http::HttpClient::new(&sandbox.config)?;
let resp = client.get(
    sandbox_rs::http::RequestOptions {
        url: "https://httpbin.org/get",
        headers: vec![],
        query: vec![],
    },
    &sandbox.config,
)?;

sandbox_rs::storage::save_response(&sandbox, Some("out.json"), &resp)?;
```
