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

# Use camera name from config
vapx info entrance

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
    config.rs          # vapx config — config management
  vapix/
    auth.rs            # Digest/Basic auth negotiation
    client.rs          # VapixClient (HTTP POST/GET with auth + validation)
    device.rs          # basicdeviceinfo.cgi wrapper
    firmware.rs        # firmwaremanagement.cgi wrapper
    applications.rs    # ACAP list/control (XML parsing)
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

Planned:

- [ ] PTZ Control (`com/ptz.cgi`)
- [ ] Parameter Management (`param.cgi`)
- [ ] User Management / Password (`pwdgrp.cgi`)
- [ ] Network Configuration (`network_settings.cgi`)
- [ ] Time & NTP (`ntp.cgi`, `timeservice.cgi`)
- [ ] I/O Port Management (`io/portmanagement.cgi`)
- [ ] Light Control (`lightcontrol.cgi`)
- [ ] Network Settings
- [ ] Time/NTP Configuration
- [ ] User/Password Management
- [ ] Batch operations (parallel, multi-camera)

## License

MIT
