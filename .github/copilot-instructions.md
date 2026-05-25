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
      snap.rs            # vapx snap — JPEG snapshot (+ time-lapse)
      fw.rs              # vapx fw — firmware management (status/upgrade/commit/rollback/reboot/check)
      acap.rs            # vapx acap — ACAP application management
      ptz.rs             # vapx ptz — PTZ control
      param.rs           # vapx param — parameter management
      user.rs            # vapx user — user account management
      pass.rs            # vapx pass — password management
      net.rs             # vapx net — network configuration
      time.rs            # vapx time — NTP/timezone management
      hw.rs              # vapx hw — I/O port management
      events.rs          # vapx events — real-time event streaming (WebSocket)
      batch.rs           # vapx batch — parallel multi-camera operations
      discover.rs        # vapx discover — API discovery
      diff.rs            # vapx diff — parameter diff between cameras (+ group diff)
      backup.rs          # vapx backup — parameter backup/restore
      overlay.rs         # vapx overlay — text/image overlay management
      log.rs             # vapx log — system/access log viewer
      stream.rs          # vapx stream — RTSP/MJPEG/snapshot URL builder
      template.rs        # vapx template — desired-state parameter templates
      audit.rs           # vapx audit — security posture audit
      cert.rs            # vapx cert — certificate management
      watch.rs           # vapx watch — multi-camera event monitoring
      rule.rs            # vapx rule — action rule management
      storage.rs         # vapx storage — SD card/edge storage management
      health.rs          # vapx health — fleet health check
      config.rs          # vapx config — config file + keyring management
      temp.rs            # vapx temp — temperature sensor readings
      daynight.rs        # vapx daynight — IR-cut filter mode
      imaging.rs         # vapx imaging — image sensor settings
      light.rs           # vapx light — IR illuminator status
      vmd.rs             # vapx vmd — video motion detection
      audio.rs           # vapx audio — audio source configuration
      mqtt.rs            # vapx mqtt — MQTT client management
      streamstatus.rs    # vapx streamstatus — stream status
      selftest.rs        # vapx selftest — device self-test
      signedvideo.rs     # vapx signedvideo — signed video management
      zipstream.rs       # vapx zipstream — ZipStream compression
      viewarea.rs        # vapx viewarea — view area management
    vapix/
      mod.rs
      auth.rs            # Digest/Basic auth auto-negotiation
      client.rs          # VapixClient with response body validation
      device.rs          # basicdeviceinfo.cgi
      firmware.rs        # firmwaremanagement.cgi
      applications.rs    # ACAP application list/control (XML)
      ptz.rs             # PTZ control (com/ptz.cgi)
      params.rs          # Parameter management (param.cgi)
      users.rs           # User management (pwdgrp.cgi)
      time.rs            # Time/NTP configuration (param.cgi root.Time)
      io.rs              # I/O port configuration (param.cgi root.IOPort)
      network.rs         # Network configuration (param.cgi root.Network)
      events.rs          # WebSocket event streaming
      discover.rs        # API discovery (apidiscovery.cgi)
      overlay.rs         # Dynamic overlay management (dynamicoverlay.cgi)
      certs.rs           # Certificate management (certificate.cgi)
      rules.rs           # Action rule management (action.cgi)
      storage.rs         # Disk/storage management (disks/, record/)
      temperature.rs     # Temperature sensor readings (temperaturecontrol.cgi)
      image.rs           # Image-related params (daynight, imaging, light, vmd, audio)
      mqtt.rs            # MQTT client management (mqtt/client.cgi, mqtt/event.cgi)
      streamstatus.rs    # Stream status (streamstatus.cgi / param.cgi fallback)
      selftest.rs        # Device self-test (deviceselftest.cgi)
      signedvideo.rs     # Signed video (signedvideo.cgi)
      zipstream.rs       # ZipStream compression (zipstream/*.cgi)
      viewarea.rs        # View area management (viewarea/info.cgi)
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
3. OS keyring lookup by camera name (if `--features keyring`)
4. Interactive prompt (if TTY)

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
- [x] Retry with exponential backoff (3 attempts on 5xx/timeout)
- [x] Per-command timeout defaults (120s for firmware, config-based for others)
- [x] `--timeout` per-command override flag

### Priority 2 — More subcommands
- [x] `vapx fw` — firmware management (status/upgrade/commit/rollback/reboot/factory-default)
- [x] `vapx acap` — list, start/stop, restart, remove
- [x] `vapx snap` — JPEG snapshot to file
- [x] `vapx ptz` — pan/tilt/zoom control
- [x] `vapx param` — parameter list/get/set
- [x] `vapx user` — user account management (list, add, update, remove)
- [x] `vapx pass` — password management
- [x] `vapx net` — network configuration (show, set)
- [x] `vapx time` — NTP/timezone
- [x] `vapx hw` — I/O ports, lights
- [x] `vapx events` — real-time event streaming (WebSocket)
- [x] `vapx discover` — API discovery
- [x] `vapx diff` — parameter diff between two cameras
- [x] `vapx backup` — parameter backup/restore
- [x] `vapx overlay` — text/image overlay management

### Priority 3 — Batch & UX
- [x] `vapx batch` — run command on multiple cameras (parallel with rayon)
- [x] Progress bars (indicatif) for batch and firmware operations
- [x] Shell completions (`vapx completions bash|zsh|fish`)
- [x] Man page generation (clap_mangen)
- [x] Output filtering (`--filter key1,key2`) for extracting specific JSON fields

### Priority 4 — Config enhancements
- [x] `vapx config add` with connectivity verification
- [x] Config profiles (`profiles:` section in cameras.yaml, `--profile` flag)
- [x] OS keyring secrets backend (optional, `--features keyring`)

### Priority 5 — Fleet management & advanced features
- [x] `vapx log` — system/access log viewer
- [x] `vapx stream` — RTSP/MJPEG/snapshot URL builder
- [x] `vapx snap --interval/--count` — time-lapse snapshots
- [x] `vapx template` — desired-state parameter templates (create/apply/diff)
- [x] `vapx audit` — security posture audit
- [x] `vapx cert` — certificate management (list/self-sign/CSR/remove)
- [x] `vapx watch` — multi-camera event monitoring (threaded)
- [x] `vapx fw check` — firmware version comparison
- [x] `vapx diff --group-diff` — diff reference camera against entire group
- [x] Output formats (`--format table|csv|yaml`) for flexible output rendering

### Priority 6 — Camera automation & operations
- [x] `vapx rule` — action rule management (list/info/enable/disable/remove/templates)
- [x] `vapx storage` — SD card and edge storage management (disks/recordings/params)
- [x] `vapx health` — fleet health check (parallel, model/firmware/latency/issues)

### Priority 7 — Camera inspection & sensors
- [x] `vapx temp` — temperature sensor readings (temperaturecontrol.cgi)
- [x] `vapx daynight` — IR-cut filter mode (ImageSource.I0.DayNight params)
- [x] `vapx imaging` — image sensor settings (ImageSource.I0.Sensor params)
- [x] `vapx light` — IR illuminator status and intensity (LightControl params)
- [x] `vapx vmd` — video motion detection configuration (Motion params)
- [x] `vapx audio` — audio source configuration (AudioSource params)
