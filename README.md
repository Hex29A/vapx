# vapx

Axis camera management CLI via VAPIX. Single static binary, no runtime dependencies.

**Not affiliated with Axis Communications AB. VAPIX is a trademark of Axis Communications AB.**

## Install

Download a release binary for your platform, or build from source:

```sh
cargo build --release
sudo cp target/release/vapx /usr/local/bin/
```

Static musl build (Linux):

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Usage

```
vapx <command> <host> [options]
```

The `host` argument can be an IP address, hostname, or a camera name defined in `cameras.yaml`.

### Commands

| Command | Description |
|---------|-------------|
| `info` | Device info (model, firmware, serial, architecture) |
| `snap` | JPEG snapshot to file (supports time-lapse) |
| `fw` | Firmware management (status, upgrade, commit, rollback, reboot, factory-default, check) |
| `acap` | ACAP application management (list, start, stop, restart, remove) |
| `ptz` | PTZ control (move, goto, preset, query, info) |
| `param` | Parameter management (list, get, set) |
| `user` | User account management (list, add, update, remove) |
| `pass` | Change user password |
| `net` | Network configuration (show, set) |
| `time` | Time/NTP configuration (show, set) |
| `hw` | I/O port management (show, set, trigger) |
| `events` | Stream real-time events via WebSocket |
| `batch` | Run command on multiple cameras in parallel |
| `discover` | Discover supported APIs on the camera |
| `diff` | Compare parameters between two cameras (or group diff) |
| `backup` | Backup and restore camera parameters |
| `overlay` | Manage text/image overlays |
| `log` | View system/access logs |
| `stream` | Generate stream URLs (RTSP, MJPEG, snapshot) |
| `template` | Apply/create parameter templates (desired-state config) |
| `audit` | Security posture audit |
| `cert` | Certificate management (list, self-sign, CSR, remove) |
| `watch` | Watch events from multiple cameras |
| `rule` | Action rule management (list, info, enable, disable, remove, templates) |
| `storage` | Storage and SD card management (list, health, recordings, params) |
| `health` | Fleet health check (parallel, model/firmware/latency/issues) |
| `temp` | Temperature sensor readings |
| `daynight` | Day/night IR-cut filter mode |
| `imaging` | Image sensor settings (brightness, contrast, exposure, WDR) |
| `light` | IR illuminator status and intensity |
| `vmd` | Video motion detection configuration |
| `audio` | Audio source configuration |
| `clip` | Audio clip management (list, play, upload, delete) |
| `mqtt` | MQTT client management (status, configure, enable, disable, events) |
| `streamstatus` | Stream status and parameters |
| `selftest` | Device self-test (preview mode only) |
| `signedvideo` | Signed video management (status, enable, disable) |
| `zipstream` | ZipStream compression profiles (status, set) |
| `viewarea` | View area management (list, get, set geometry) |
| `config` | Manage cameras.yaml (path, check, list, init, add) |
| `completions` | Generate shell completions (bash, zsh, fish) |
| `mangen` | Generate man pages |

### Examples

```sh
# Get device info as JSON
vapx info 192.168.7.10 -u martincr -p secret

# Plain text output
vapx info 192.168.7.10 -u martincr -p secret --plain

# Specific properties only
vapx info 192.168.7.10 -u martincr -p secret --props Brand,Version,Architecture

# Take a snapshot
vapx snap 192.168.7.10 -u martincr -p secret -o photo.jpg
vapx snap 192.168.7.10 -u martincr -p secret --resolution 1920x1080 --compression 25

# Check firmware status
vapx fw 192.168.7.10 -u martincr -p secret
vapx fw 192.168.7.10 -u martincr -p secret --plain

# List installed ACAP applications
vapx acap list 192.168.7.10 -u martincr -p secret
vapx acap list 192.168.7.10 -u martincr -p secret --plain

# Control ACAP applications (package name is positional)
vapx acap start 192.168.7.10 vdo_larod -u martincr -p secret
vapx acap stop 192.168.7.10 vdo_larod -u martincr -p secret
vapx acap restart 192.168.7.10 vdo_larod -u martincr -p secret
vapx acap remove 192.168.7.10 vdo_larod -u martincr -p secret

# PTZ control
vapx ptz move 192.168.7.10 home -u martincr -p secret
vapx ptz goto 192.168.7.10 --pan 90.0 --tilt -20.0 --zoom 5000 -u martincr -p secret
vapx ptz goto 192.168.7.10 --rpan 10.0 --speed 50 -u martincr -p secret
vapx ptz preset 192.168.7.10 "Door" -u martincr -p secret
vapx ptz query 192.168.7.10 position -u martincr -p secret
vapx ptz query 192.168.7.10 limits --plain -u martincr -p secret
vapx ptz info 192.168.7.10 -u martincr -p secret

# Parameter management
vapx param list 192.168.7.10 --group root.Brand -u martincr -p secret
vapx param get 192.168.7.10 root.Brand.Brand -u martincr -p secret
vapx param set 192.168.7.10 root.Network.HostName=myaxis -u martincr -p secret

# User management
vapx user list 192.168.7.10 -u martincr -p secret
vapx user add 192.168.7.10 --name viewer1 --pwd secret --role viewer -u martincr -p secret
vapx user add 192.168.7.10 --name op1 --pwd secret --role operator --ptz -u martincr -p secret
vapx user update 192.168.7.10 --name viewer1 --pwd newpass -u martincr -p secret
vapx user remove 192.168.7.10 --name viewer1 -u martincr -p secret

# Use camera name from config
vapx info entrance

# Temperature sensor readings
vapx temp 192.168.7.10 -u admin -p secret
vapx temp entrance --format table

# Day/night mode
vapx daynight 192.168.7.10 -u admin -p secret

# Image sensor settings
vapx imaging 192.168.7.10 -u admin -p secret

# IR illuminator status
vapx light 192.168.7.10 -u admin -p secret

# Video motion detection config
vapx vmd 192.168.7.10 -u admin -p secret

# Audio source configuration
vapx audio 192.168.7.10 -u admin -p secret

# Audio clip management
vapx clip list 192.168.7.10 -u admin -p secret
vapx clip play 192.168.7.10 siren -u admin -p secret
vapx clip upload 192.168.7.10 /path/to/alert.wav -u admin -p secret
vapx clip upload 192.168.7.10 /path/to/alert.wav --name warning -u admin -p secret
vapx clip delete 192.168.7.10 siren -u admin -p secret

# MQTT client management
vapx mqtt status 192.168.7.10 -u admin -p secret
vapx mqtt enable 192.168.7.10 -u admin -p secret
vapx mqtt disable 192.168.7.10 -u admin -p secret
vapx mqtt configure 192.168.7.10 --broker mqtt.example.com --broker-port 1883 -u admin -p secret
vapx mqtt events 192.168.7.10 -u admin -p secret

# Stream status
vapx streamstatus 192.168.7.10 -u admin -p secret

# Device self-test (requires preview mode)
vapx selftest 192.168.7.10 -u admin -p secret

# Signed video
vapx signedvideo status 192.168.7.10 -u admin -p secret
vapx signedvideo enable 192.168.7.10 -u admin -p secret

# ZipStream compression
vapx zipstream status 192.168.7.10 -u admin -p secret
vapx zipstream set 192.168.7.10 --profile classic --level 1 -u admin -p secret

# View areas
vapx viewarea list 192.168.7.10 -u admin -p secret
vapx viewarea get 192.168.7.10 --id 1000001 -u admin -p secret

# Show config file location
vapx config path

# Validate config
vapx config check

# List configured cameras
vapx config list

# Create template config
vapx config init

# Change password
vapx pass 192.168.7.10 -u root -p oldpass --name root --pwd newpass

# Network configuration
vapx net show 192.168.7.10 -u admin -p secret
vapx net set 192.168.7.10 root.Network.HostName=myaxis -u admin -p secret

# Time/NTP configuration
vapx time show 192.168.7.10 -u admin -p secret
vapx time set 192.168.7.10 --ntp-server pool.ntp.org -u admin -p secret

# I/O port management
vapx hw show 192.168.7.10 -u admin -p secret
vapx hw trigger 192.168.7.10 --index 0 --state active -u admin -p secret
vapx hw trigger 192.168.7.10 --index 0 --state inactive -u admin -p secret
vapx hw trigger 192.168.7.10 --index 0 --state active --pulse 500 -u admin -p secret

# Stream real-time events via WebSocket
vapx events 192.168.7.10 -u admin -p secret

# Run a command on multiple cameras in parallel
vapx batch info cam1 cam2 cam3 -u admin -p secret
vapx batch fw building_a -u admin -p secret

# Discover supported APIs
vapx discover 192.168.7.10 -u admin -p secret

# Compare parameters between two cameras
vapx diff 192.168.7.10 192.168.7.11 -u admin -p secret
vapx diff 192.168.7.10 --group-diff building_a -u admin -p secret

# Backup/restore parameters
vapx backup save 192.168.7.10 -u admin -p secret -o backup.json
vapx backup restore 192.168.7.10 -u admin -p secret -i backup.json --dry-run

# Overlay management
vapx overlay list 192.168.7.10 -u admin -p secret

# View system/access logs
vapx log system 192.168.7.10 -u admin -p secret
vapx log access 192.168.7.10 -u admin -p secret

# Generate stream URLs
vapx stream rtsp 192.168.7.10
vapx stream mjpeg 192.168.7.10 --resolution 1920x1080

# Parameter templates (desired-state config)
vapx template create 192.168.7.10 --groups root.Network,root.Time -u admin -p secret -o template.json
vapx template apply 192.168.7.10 -u admin -p secret -i template.json --dry-run
vapx template diff 192.168.7.10 -u admin -p secret -i template.json

# Security audit
vapx audit 192.168.7.10 -u admin -p secret
vapx audit 192.168.7.10 -u admin -p secret --plain

# Certificate management
vapx cert list 192.168.7.10 -u admin -p secret

# Watch events from multiple cameras
vapx watch cam1 cam2 cam3 -u admin -p secret

# Action rule management
vapx rule list 192.168.7.10 -u admin -p secret
vapx rule templates 192.168.7.10 -u admin -p secret

# Storage/SD card management
vapx storage list 192.168.7.10 -u admin -p secret
vapx storage recordings 192.168.7.10 -u admin -p secret
vapx storage health 192.168.7.10 -u admin -p secret

# Fleet health check
vapx health cam1 cam2 cam3 -u admin -p secret
vapx health building_a -u admin -p secret

# Shell completions
vapx completions bash > ~/.local/share/bash-completion/completions/vapx
vapx completions zsh > ~/.zfunc/_vapx
vapx completions fish > ~/.config/fish/completions/vapx.fish

# Generate man pages
vapx mangen /usr/local/share/man/man1/
```

### Global Options

| Flag | Description |
|------|-------------|
| `-v` / `-vv` / `-vvv` | Verbosity level (info / debug / trace) |
| `--filter` | Filter output fields (comma-separated, e.g. `--filter model,serial`) |
| `--format` | Output format: `json` (default), `table`, `csv`, `yaml` |
| `--profile` | Config profile (from `cameras.yaml` profiles section) |
| `--config` | Path to cameras.yaml config file |

### Per-command Options

| Flag | Description |
|------|-------------|
| `-u, --user` | Username (or set `VAPX_USER`) |
| `-p, --pass` | Password (or set `VAPX_PASS`) |
| `-k, --insecure` | Skip TLS cert verification |
| `--port` | Override port (default: 80/443) |
| `--timeout` | Request timeout in seconds |
| `--plain` | Output plain text instead of JSON |

## Configuration

Config file search order:

1. `$VAPX_CONFIG` environment variable
2. `./cameras.yaml` (current directory)
3. `~/.config/vapx/cameras.yaml` (Linux/macOS XDG)

### cameras.yaml

```yaml
defaults:
  user: root
  https: false
  verify_ssl: true
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
    verify_ssl: true
    port: 443

groups:
  building_a:
    - entrance
    - parking
```

#### `defaults` / per-camera fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `user` | string | `root` | VAPIX username |
| `pass` | string | — | Password. Supports `${ENV_VAR}` substitution |
| `https` | bool | `false` | Use HTTPS instead of HTTP |
| `verify_ssl` | bool | `true` | Verify TLS certificate (only applies when `https: true`; use `-k`/`--insecure` to disable per-command) |
| `port` | int | 80 / 443 | Override port (useful for non-standard setups) |
| `timeout` | int (s) | `10` | Request timeout in seconds |
| `fw_timeout` | int (s) | `300` | Firmware upload timeout in seconds (overrides `timeout` for `fw upgrade`) |
| `enabled` | bool | `true` | Set `false` to skip camera in `batch`/`watch`/`health` without removing it |

Per-camera fields override `defaults`. Each camera entry also requires `host`.

#### `profiles`

Named sets of defaults, selectable with `--profile <name>`. Supports the same fields as `defaults`.

#### `groups`

Named lists of camera names for use with `vapx batch`, `vapx watch`, `vapx health`.

### Secrets

Passwords use `${ENV_VAR}` substitution. Set them via:

- Shell exports: `export ENTRANCE_PASS=secret`
- A `.env` file (source it before running vapx)
- CI/CD secrets

### Credential Resolution Order

1. CLI flags (`-u`, `-p`)
2. `cameras.yaml` lookup (by name or host)
3. OS keyring lookup (if built with `--features keyring`)
4. Interactive prompt (TTY only)

## Authentication

vapx auto-negotiates authentication:

- **HTTP**: Digest access authentication (challenge-response, no plaintext)
- **HTTPS**: Basic access authentication (safe over TLS)

Per [VAPIX documentation](https://developer.axis.com/vapix/authentication/).

## Building

### Targets

| Platform | Target | Command |
|----------|--------|---------|
| Linux x86_64 | `x86_64-unknown-linux-musl` | `cargo build --release --target x86_64-unknown-linux-musl` |
| Linux ARM64 (RPi 4/5) | `aarch64-unknown-linux-musl` | `cross build --release --target aarch64-unknown-linux-musl` |
| Linux ARMv7 (RPi 3) | `armv7-unknown-linux-musleabihf` | `cross build --release --target armv7-unknown-linux-musleabihf` |
| macOS Intel | `x86_64-apple-darwin` | `cargo build --release --target x86_64-apple-darwin` |
| macOS Apple Silicon | `aarch64-apple-darwin` | `cargo build --release --target aarch64-apple-darwin` |

ARM Linux targets use [cross](https://github.com/cross-rs/cross) for musl cross-compilation.

### Release profile

Binaries are optimized for size: LTO, single codegen unit, stripped, panic=abort.

## Testing

```sh
# Unit tests (no camera needed)
cargo test --lib

# Full integration tests (requires reachable camera)
VAPX_TEST_HOST=192.168.7.10 VAPX_TEST_USER=martincr VAPX_TEST_PASS=secret cargo test

# Integration tests skip gracefully if camera is unreachable
cargo test --test integration
```

## Architecture

```
src/
  main.rs              # CLI parsing, subcommand dispatch
  cmd/
    info.rs            # vapx info — device identification
    snap.rs            # vapx snap — JPEG snapshot (+ time-lapse)
    fw.rs              # vapx fw — firmware management
    acap.rs            # vapx acap — ACAP app management
    ptz.rs             # vapx ptz — PTZ control
    param.rs           # vapx param — parameter management
    user.rs            # vapx user — user account management
    pass.rs            # vapx pass — password management
    net.rs             # vapx net — network configuration
    time.rs            # vapx time — NTP/timezone
    hw.rs              # vapx hw — I/O port management
    events.rs          # vapx events — real-time event streaming
    batch.rs           # vapx batch — parallel multi-camera operations
    discover.rs        # vapx discover — API discovery
    diff.rs            # vapx diff — parameter diff between cameras
    backup.rs          # vapx backup — parameter backup/restore
    overlay.rs         # vapx overlay — overlay management
    log.rs             # vapx log — system/access log viewer
    stream.rs          # vapx stream — RTSP/MJPEG/snapshot URL builder
    template.rs        # vapx template — desired-state parameter templates
    audit.rs           # vapx audit — security posture audit
    cert.rs            # vapx cert — certificate management
    watch.rs           # vapx watch — multi-camera event monitoring
    rule.rs            # vapx rule — action rule management
    storage.rs         # vapx storage — SD card/edge storage management
    health.rs          # vapx health — fleet health check
    temp.rs            # vapx temp — temperature sensor readings
    daynight.rs        # vapx daynight — IR-cut filter mode
    imaging.rs         # vapx imaging — image sensor settings
    light.rs           # vapx light — IR illuminator status
    vmd.rs             # vapx vmd — video motion detection
    audio.rs           # vapx audio — audio source configuration
    clip.rs            # vapx clip — audio clip management (list/play/upload/delete)
    mqtt.rs            # vapx mqtt — MQTT client management
    streamstatus.rs    # vapx streamstatus — stream status
    selftest.rs        # vapx selftest — device self-test
    signedvideo.rs     # vapx signedvideo — signed video
    zipstream.rs       # vapx zipstream — ZipStream compression
    viewarea.rs        # vapx viewarea — view area management
    config.rs          # vapx config — config management
  vapix/
    auth.rs            # Digest/Basic auth negotiation
    client.rs          # VapixClient (HTTP with auth, retry, error sanitization)
    device.rs          # basicdeviceinfo.cgi
    firmware.rs        # firmwaremanagement.cgi
    applications.rs    # ACAP list/control (XML parsing)
    ptz.rs             # PTZ control (com/ptz.cgi)
    params.rs          # Parameter management (param.cgi)
    users.rs           # User management (pwdgrp.cgi)
    time.rs            # Time/NTP configuration
    io.rs              # I/O port configuration
    network.rs         # Network configuration
    events.rs          # WebSocket event streaming
    discover.rs        # API discovery (apidiscovery.cgi)
    overlay.rs         # Dynamic overlay management
    certs.rs           # Certificate management
    rules.rs           # Action rule management
    storage.rs         # Disk/storage management
    temperature.rs     # Temperature sensors
    image.rs           # Image params (daynight, imaging, light, vmd, audio)
    audio_clip.rs      # Audio clip management (list/play/upload/delete)
    mqtt.rs            # MQTT client management
    streamstatus.rs    # Stream status
    selftest.rs        # Device self-test
    signedvideo.rs     # Signed video
    zipstream.rs       # ZipStream compression
    viewarea.rs        # View area management
  config/
    cameras.rs         # cameras.yaml loading, env substitution, name resolution
    credentials.rs     # Credential resolution (flags > yaml > keyring > prompt)
  output/
    format.rs          # JSON, table, CSV, YAML formatters
tests/
  integration.rs       # Live camera integration tests (75 tests)
```

## VAPIX API Coverage

- [x] Basic Device Information (`basicdeviceinfo.cgi`)
- [x] JPEG Snapshot (`jpg/image.cgi`)
- [x] Firmware Management (`firmwaremanagement.cgi`)
- [x] ACAP Application Lifecycle (`applications/*.cgi`)
- [x] PTZ Control (`com/ptz.cgi`)
- [x] Parameter Management (`param.cgi`)
- [x] User Management (`pwdgrp.cgi`)
- [x] Network Configuration (`param.cgi root.Network`)
- [x] Time/NTP Configuration (`param.cgi root.Time`)
- [x] I/O Port Management (`param.cgi root.IOPort`, `portmanagement.cgi`)
- [x] WebSocket Event Streaming
- [x] API Discovery (`apidiscovery.cgi`)
- [x] Dynamic Overlays (`dynamicoverlay.cgi`)
- [x] Certificate Management (`certificate.cgi`)
- [x] Action Rules (`action.cgi`)
- [x] Storage/Disk Management (`disks/*.cgi`, `record/*.cgi`)
- [x] Temperature Sensors (`temperaturecontrol.cgi`)
- [x] Image Source Parameters (day/night, imaging, light, VMD, audio)
- [x] MQTT Client (`mqtt/client.cgi`, `mqtt/event.cgi`)
- [x] Stream Status (`streamstatus.cgi`, `param.cgi` fallback)
- [x] Device Self-Test (`deviceselftest.cgi`)
- [x] Signed Video (`signedvideo.cgi`)
- [x] ZipStream Compression (`zipstream/*.cgi`)
- [x] View Area Management (`viewarea/info.cgi`)
- [x] Audio Clip Management (`/axis-cgi/audio/*.cgi`)

## License

MIT
