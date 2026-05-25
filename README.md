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
| `snap` | JPEG snapshot to file |
| `fw` | Firmware status |
| `acap` | ACAP application management (list, start, stop, restart, remove) |
| `ptz` | PTZ control (move, goto, preset, query, info) |
| `param` | Parameter management (list, get, set) |
| `user` | User account management (list, add, update, remove) |
| `temp` | Temperature sensor readings |
| `daynight` | Day/night IR-cut filter mode |
| `imaging` | Image sensor settings (brightness, contrast, exposure, WDR) |
| `light` | IR illuminator status and intensity |
| `vmd` | Video motion detection configuration |
| `audio` | Audio source configuration |
| `mqtt` | MQTT client management (status, configure, enable, disable, events) |
| `streamstatus` | Stream status and parameters |
| `selftest` | Device self-test (preview mode only) |
| `signedvideo` | Signed video management (status, enable, disable) |
| `zipstream` | ZipStream compression profiles (status, set) |
| `viewarea` | View area management (list, get, set geometry) |
| `config` | Manage cameras.yaml (path, check, list, init) |

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

# Control ACAP applications
vapx acap start 192.168.7.10 --package vdo_larod -u martincr -p secret
vapx acap stop 192.168.7.10 --package vdo_larod -u martincr -p secret
vapx acap restart 192.168.7.10 --package vdo_larod -u martincr -p secret
vapx acap remove 192.168.7.10 --package vdo_larod -u martincr -p secret

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
```

### Global Options

| Flag | Description |
|------|-------------|
| `-v` | Info-level logging |
| `-vv` | Debug-level logging |
| `-vvv` | Trace-level logging |

### Per-command Options

| Flag | Description |
|------|-------------|
| `-u, --user` | Username (or set `VAPX_USER`) |
| `-p, --pass` | Password (or set `VAPX_PASS`) |
| `-k, --insecure` | Skip TLS cert verification |
| `--port` | Override port (default: 80/443) |
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
    verify_ssl: true
    port: 443

groups:
  building_a:
    - entrance
    - parking
```

### Secrets

Passwords use `${ENV_VAR}` substitution. Set them via:

- Shell exports: `export ENTRANCE_PASS=secret`
- A `.env` file (source it before running vapx)
- CI/CD secrets

### Credential Resolution Order

1. CLI flags (`-u`, `-p`)
2. `cameras.yaml` lookup (by name or host)
3. Interactive prompt (TTY only)

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
    snap.rs            # vapx snap — JPEG snapshot
    fw.rs              # vapx fw — firmware status
    acap.rs            # vapx acap — ACAP app management
    ptz.rs             # vapx ptz — PTZ control
    param.rs           # vapx param — parameter management
    user.rs            # vapx user — user account management
    config.rs          # vapx config — config management
  vapix/
    auth.rs            # Digest/Basic auth negotiation
    client.rs          # VapixClient (HTTP POST/GET with auth + validation)
    device.rs          # basicdeviceinfo.cgi wrapper
    firmware.rs        # firmwaremanagement.cgi wrapper
    applications.rs    # ACAP list/control (XML parsing)
    ptz.rs             # PTZ control (com/ptz.cgi)
    params.rs          # Parameter management (param.cgi)
    users.rs           # User management (pwdgrp.cgi)
  config/
    cameras.rs         # cameras.yaml loading, env substitution, name resolution
    credentials.rs     # Credential resolution (flags > yaml > prompt)
  output/
    format.rs          # JSON and plain text formatters
tests/
  integration.rs       # Live camera integration tests
```

## VAPIX API Coverage

Currently implemented:

- [x] Basic Device Information (`basicdeviceinfo.cgi`)
- [x] JPEG Snapshot (`jpg/image.cgi`)
- [x] Firmware Management (`firmwaremanagement.cgi`)
- [x] ACAP Application Lifecycle (`applications/*.cgi`)
- [x] PTZ Control (`com/ptz.cgi`)
- [x] Parameter Management (`param.cgi`)
- [x] User Management (`pwdgrp.cgi`)
- [x] MQTT Client (`mqtt/client.cgi`, `mqtt/event.cgi`)
- [x] Stream Status (`streamstatus.cgi`, `param.cgi` fallback)
- [x] Device Self-Test (`deviceselftest.cgi`)
- [x] Signed Video (`signedvideo.cgi`)
- [x] ZipStream Compression (`zipstream/*.cgi`)
- [x] View Area Management (`viewarea/info.cgi`)

## License

MIT
