# vapx — Copilot Instructions

---

## Project Overview

**`vapx`** is a single Rust CLI binary for managing Axis network cameras via VAPIX. It uses a subcommand structure (like `git` or `docker`): `vapx <subcommand> <host> [options]`.

**Not affiliated with Axis Communications AB. VAPIX is a trademark of Axis Communications AB.**

---

## Architecture Decisions

| Decision | Value |
|----------|-------|
| Language | Rust stable |
| Binary | Single `vapx` binary with clap subcommands |
| Static builds | musl for Linux (x86_64, aarch64, armv7hf) |
| CLI framework | `clap` with derive macros |
| HTTP | `reqwest` blocking with `rustls-tls` (no native OpenSSL dependency) |
| Serialization | `serde` + `serde_json` + `serde_yaml` |
| Config | `cameras.yaml` with env var substitution, XDG paths |
| Auth | Digest (HTTP) / Basic (HTTPS) auto-negotiation via `digest_auth` crate |
| Error handling | `thiserror` in lib, `anyhow` in cmd/ |
| Logging | `tracing` + `tracing-subscriber` with `-v`/`-vv`/`-vvv` |
| XML parsing | `roxmltree` for ACAP responses |

---

## Project Structure

```
vapx/
  src/
    main.rs              # Cli struct, subcommand dispatch, tracing init
    cmd/
      mod.rs
      info.rs            # vapx info — device identification
      snap.rs            # vapx snap — JPEG snapshot
      fw.rs              # vapx fw — firmware status
      acap.rs            # vapx acap — ACAP application management
      config.rs          # vapx config — config file management
    vapix/
      mod.rs
      auth.rs            # Digest/Basic auth auto-negotiation
      client.rs          # VapixClient with response body validation
      device.rs          # basicdeviceinfo.cgi
      firmware.rs        # firmwaremanagement.cgi
      applications.rs    # ACAP application list/control (XML)
    config/
      mod.rs
      cameras.rs         # cameras.yaml loading, env var substitution
      credentials.rs     # Credential resolution (flags > yaml > prompt)
    output/
      mod.rs
      format.rs          # JSON and plain text formatters
  tests/
    integration.rs       # Live camera integration tests
  .github/
    workflows/ci.yml     # CI + cross-platform release builds
  Cargo.toml
  README.md
  cameras.yaml           # User config (gitignored in practice)
```

---

## Authentication

Per VAPIX documentation (https://developer.axis.com/vapix/authentication/):

- **HTTP**: Digest access authentication (challenge-response)
- **HTTPS**: Basic access authentication

Implementation in `vapix/auth.rs`:
1. Send request without auth
2. If 401 with `WWW-Authenticate: Digest` → compute digest response and retry
3. If HTTPS → use Basic auth directly

---

## VAPIX API Patterns

### Modern APIs (JSON POST)
```
POST /axis-cgi/basicdeviceinfo.cgi
Content-Type: application/json
{"apiVersion": "1.0", "method": "getAllProperties"}
```

Response validation: HTTP 200 can still contain errors in JSON body:
```json
{"apiVersion": "1.0", "error": {"code": 1000, "message": "..."}}
```
The client checks for this.

### Legacy APIs (CGI GET)
```
GET /axis-cgi/com/ptz.cgi?pan=30&tilt=-10&zoom=500
GET /axis-cgi/jpg/image.cgi?resolution=1920x1080
```

### ACAP (XML responses)
```
GET /axis-cgi/applications/list.cgi → XML, parsed with roxmltree
```

---

## Config Format (cameras.yaml)

```yaml
defaults:
  user: root
  https: false
  verify_ssl: false
  timeout: 10

cameras:
  entrance:
    host: 192.168.1.100
    pass: "${ENTRANCE_PASS}"
  parking:
    host: 192.168.1.101
    user: admin
    pass: "${PARKING_PASS}"
    https: true

groups:
  building_a:
    - entrance
    - parking
```

Features:
- Name-based resolution: `vapx info entrance` resolves via config
- `${ENV_VAR}` substitution at load time
- Defaults inherited by all cameras
- Groups for batch operations
- Search order: `$VAPX_CONFIG` → `./cameras.yaml` → `~/.config/vapx/cameras.yaml`

---

## Credential Resolution (config/credentials.rs)

Priority order:
1. Explicit `-u`/`-p` CLI flags
2. `cameras.yaml` lookup by name or host
3. Interactive prompt (if TTY)

The `host` argument resolves through config: if it matches a camera name, use that entry's host/credentials.

---

## Build Targets

| Platform | Target | Method |
|----------|--------|--------|
| Linux x86_64 | `x86_64-unknown-linux-musl` | `cargo build` with musl-tools |
| Linux ARM64 (RPi 4/5) | `aarch64-unknown-linux-musl` | `cross build` |
| Linux ARMv7 (RPi 3/Zero2) | `armv7-unknown-linux-musleabihf` | `cross build` |
| macOS Intel | `x86_64-apple-darwin` | native `cargo build` |
| macOS Apple Silicon | `aarch64-apple-darwin` | native `cargo build` |

CI builds all targets on push to main via GitHub Actions.

---

## Testing

```sh
# Unit tests only (no camera)
cargo test --lib

# Full suite including integration (needs camera)
VAPX_TEST_HOST=192.168.7.10 VAPX_TEST_USER=martincr VAPX_TEST_PASS=avhsroot cargo test

# Integration tests skip gracefully if camera unreachable
cargo test --test integration
```

Test camera: AXIS Q1615 Mk III (192.168.7.10), firmware 12.9.57, armv7hf, Artpec-7

---

## Subcommand Implementation Pattern

```rust
use clap::Args;
use crate::config::credentials::resolve;
use crate::vapix::client::VapixClient;

#[derive(Args)]
pub struct XxxCmd {
    pub host: String,
    #[arg(short, long, env = "VAPX_USER")]
    pub user: Option<String>,
    #[arg(short, long, env = "VAPX_PASS")]
    pub pass: Option<String>,
    #[arg(short = 'k', long)]
    pub insecure: bool,
    #[arg(long)]
    pub port: Option<u16>,
    #[arg(long)]
    pub plain: bool,
}

impl XxxCmd {
    pub fn run(self) -> anyhow::Result<()> {
        let (creds, resolved_host) = resolve(
            &self.host, self.user.as_deref(), self.pass.as_deref(),
            self.port, self.insecure,
        )?;
        let client = VapixClient::new(&resolved_host, creds.port, creds, 10);
        // ... use client ...
        Ok(())
    }
}
```

---

## TODO (Implementation Roadmap)

### Priority 1 — Core robustness
- [ ] Retry with exponential backoff (3 attempts on 5xx/timeout)
- [ ] Per-command timeout defaults (120s for firmware, 30s for ACAP upload)
- [ ] `--timeout` global override flag

### Priority 2 — More subcommands
- [x] `vapx fw` — firmware status
- [x] `vapx acap` — list, start/stop, restart, remove
- [x] `vapx snap` — JPEG snapshot to file
- [ ] `vapx ptz` — pan/tilt/zoom control
- [ ] `vapx pass` — user/password management
- [ ] `vapx net` — network configuration
- [ ] `vapx time` — NTP/timezone
- [ ] `vapx hw` — I/O ports, lights

### Priority 3 — Batch & UX
- [ ] `vapx batch` — run command on multiple cameras (parallel with rayon)
- [ ] Progress bars (indicatif) for batch and firmware operations
- [ ] Shell completions (`vapx completions bash|zsh|fish`)
- [ ] Man page generation (clap_mangen)

### Priority 4 — Config enhancements
- [ ] `vapx config add` with connectivity verification
- [ ] OS keyring secrets backend (optional)
