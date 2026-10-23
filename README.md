# <img src="docs/logo.svg" width="44" height="32" alt=""> vapx

Manage Axis network cameras from the command line: firmware, users, ACAP, network, batch jobs across many cameras. One static binary, no runtime. Install from [vapx.29a.se](https://vapx.29a.se).

**Not affiliated with Axis Communications AB. VAPIX is a trademark of Axis Communications AB.**

![vapx config add registering a camera, then vapx config list](docs/demo/setup.gif)

![vapx info showing a camera's model, firmware and serial number](docs/demo/info.gif)

![vapx reading system readiness, temperature, day/night settings and available APIs](docs/demo/sensors.gif)

## One tool: CLI and MCP server

vapx manages Axis cameras from the command line, and as `vapx mcp` it is also the MCP server that lets an AI assistant look at and operate your cameras. Both use the same `cameras.yaml` and the same VAPIX code, so a camera you add with `vapx config add` is available to both, and a fix is made once.

vapx is for: firmware upgrades, network configuration, user management, ACAP installation, batch operations across many cameras, raw `param.cgi` access. `vapx mcp` is for: snapshots, PTZ, I/O, overlays, analytics, event monitoring, temperature, stream diagnostics, rebooting, log reading, fleet health and audits, driven by an assistant. See [MCP server](#mcp-server).

It replaces the separate Python server [vapx-mcp](https://github.com/Hex29A/vapx-mcp), which reimplemented VAPIX on its own. If you use it, see [Migrating from the Python vapx-mcp](#migrating-from-the-python-vapx-mcp).

## Install

**Linux and macOS** (x86_64, ARM64, ARMv7 / Raspberry Pi):

```sh
curl -fsSL https://vapx.29a.se/install | sh
```

**Windows** (PowerShell):

```powershell
irm https://vapx.29a.se/install.ps1 | iex
```

**Homebrew** (macOS, Linux):

```sh
brew install hex29a/tap/vapx
```

Piping a script from the internet into a shell asks you to trust it. Both scripts are short
and meant to be read first — drop the `| sh` / `| iex` to see what you would run, or download,
read, then run:

```sh
curl -fsSL -o install.sh https://vapx.29a.se/install
less install.sh
sh install.sh
```

What they do: pick the release binary for your OS and CPU, download it together with
`SHA256SUMS` from the GitHub release, refuse to install if the checksum does not match, and put
`vapx` in `~/.local/bin` (Windows: `%LOCALAPPDATA%\Programs\vapx`, added to your user PATH). No
sudo or admin. Nothing else is touched — your `cameras.yaml` stays where it is.

| | Linux / macOS | Windows |
|---|---|---|
| Update | run the same command again | run the same command again |
| Specific version | `curl … \| VAPX_VERSION=0.27.0 sh` | `$env:VAPX_VERSION="0.27.0"; irm … \| iex` |
| Other directory | `curl … \| VAPX_INSTALL_DIR=/opt/bin sh` | `$env:VAPX_INSTALL_DIR="C:\tools"; irm … \| iex` |
| Uninstall | `curl … \| sh -s -- --uninstall` | `$env:VAPX_UNINSTALL=1; irm … \| iex` |

The scripts work from v0.27.0, the first release that publishes `SHA256SUMS` and macOS and
Windows binaries.

**Manual download:** every [release](https://github.com/Hex29A/vapx/releases) has a single
static binary per platform plus `SHA256SUMS`. Download, `chmod +x`, put it on your PATH.

**From source** (any platform with Rust):

```sh
cargo build --release
cp target/release/vapx ~/.local/bin/
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
| `ssh` | SSH service on/off and SSH accounts (status, enable, disable, users list/add/passwd/remove) |
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
| `cert` | Certificate management (list, info, self-sign, CSR, remove). On AXIS OS 12+ all of them use the certificate REST API (`/config/rest/cert/v1`); cameras without it are asked through the legacy API. `self-sign` and `remove` change the camera |
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
| `mqtt` | MQTT client management (status, configure, enable, disable, event publication filters) |
| `streamstatus` | Stream status and parameters |
| `selftest` | Device self-test (preview mode only) |
| `signedvideo` | Signed video management (status, enable, disable) |
| `zipstream` | ZipStream compression profiles (status, set) |
| `viewarea` | View area management (list, get, set geometry) |
| `config` | Manage cameras.yaml (path, check, list, init, add, rename, group) |
| `enroll` | Set up a factory-default camera: create the initial account and add it to cameras.yaml |
| `systemready` | Device readiness and factory-default state (works without credentials) |
| `mcp` | MCP server for AI assistants: read-only camera tools over stdio or Streamable HTTP (see [MCP server](#mcp-server)) |
| `completions` | Generate shell completions (bash, zsh, fish) |
| `mangen` | Generate man pages |

### Examples

```sh
# Get device info as JSON
vapx info 192.168.1.100 -u root -p <password>

# Plain text output
vapx info 192.168.1.100 -u root -p <password> --plain

# Specific properties only
vapx info 192.168.1.100 -u root -p <password> --props Brand,Version,Architecture

# Take a snapshot
vapx snap 192.168.1.100 -u root -p <password> -o photo.jpg
vapx snap 192.168.1.100 -u root -p <password> --resolution 1920x1080 --compression 25

# Check firmware status
vapx fw 192.168.1.100 -u root -p <password>
vapx fw 192.168.1.100 -u root -p <password> --plain

# List installed ACAP applications
vapx acap list 192.168.1.100 -u root -p <password>
vapx acap list 192.168.1.100 -u root -p <password> --plain

# Control ACAP applications (package name is positional)
vapx acap start 192.168.1.100 vdo_larod -u root -p <password>
vapx acap stop 192.168.1.100 vdo_larod -u root -p <password>
vapx acap restart 192.168.1.100 vdo_larod -u root -p <password>
vapx acap remove 192.168.1.100 vdo_larod -u root -p <password>

# PTZ control
vapx ptz move 192.168.1.100 home -u root -p <password>
vapx ptz goto 192.168.1.100 --pan 90.0 --tilt -20.0 --zoom 5000 -u root -p <password>
vapx ptz goto 192.168.1.100 --rpan 10.0 --speed 50 -u root -p <password>
vapx ptz preset 192.168.1.100 "Door" -u root -p <password>
vapx ptz query 192.168.1.100 position -u root -p <password>
vapx ptz query 192.168.1.100 limits --plain -u root -p <password>
vapx ptz info 192.168.1.100 -u root -p <password>

# Parameter management
vapx param list 192.168.1.100 --group root.Brand -u root -p <password>
vapx param get 192.168.1.100 root.Brand.Brand -u root -p <password>
vapx param set 192.168.1.100 root.Network.HostName=myaxis -u root -p <password>

# User management
vapx user list 192.168.1.100 -u root -p <password>
vapx user add 192.168.1.100 --name viewer1 --pwd secret --role viewer -u root -p <password>
vapx user add 192.168.1.100 --name op1 --pwd secret --role operator --ptz -u root -p <password>
vapx user update 192.168.1.100 --name viewer1 --pwd newpass -u root -p <password>
vapx user remove 192.168.1.100 --name viewer1 -u root -p <password>

# SSH service and SSH accounts (accounts need AXIS OS 11.5+; `ssh status` shows how the device logs in)
vapx ssh status 192.168.1.100 -u root -p <password>
vapx ssh enable 192.168.1.100 -u root -p <password>
vapx ssh users add 192.168.1.100 --name support --pwd secret -u root -p <password>
vapx ssh users passwd 192.168.1.100 --name support --pwd newpass -u root -p <password>
vapx ssh users list 192.168.1.100 -u root -p <password>
vapx ssh users remove 192.168.1.100 --name support -u root -p <password>
vapx ssh disable 192.168.1.100 -u root -p <password>

# Use camera name from config
vapx info entrance

# Temperature sensor readings
vapx temp 192.168.1.100 -u admin -p secret
vapx temp entrance --format table

# Day/night mode
vapx daynight 192.168.1.100 -u admin -p secret

# Image sensor settings
vapx imaging 192.168.1.100 -u admin -p secret

# IR illuminator status
vapx light 192.168.1.100 -u admin -p secret

# Video motion detection config
vapx vmd 192.168.1.100 -u admin -p secret

# Audio source configuration
vapx audio 192.168.1.100 -u admin -p secret

# Audio clip management
vapx clip list 192.168.1.100 -u admin -p secret
vapx clip play 192.168.1.100 siren -u admin -p secret
vapx clip upload 192.168.1.100 /path/to/alert.wav -u admin -p secret
vapx clip upload 192.168.1.100 /path/to/alert.wav --name warning -u admin -p secret
vapx clip delete 192.168.1.100 siren -u admin -p secret

# MQTT client management
vapx mqtt status 192.168.1.100 -u admin -p secret
vapx mqtt enable 192.168.1.100 -u admin -p secret
vapx mqtt disable 192.168.1.100 -u admin -p secret
vapx mqtt configure 192.168.1.100 --broker mqtt.example.com --broker-port 1883 -u admin -p secret
vapx mqtt events 192.168.1.100 -u admin -p secret
# Set event publication filters (read-modify-write; only eventFilterList is changed).
# Axis only starts publishing once at least one filter is set.
vapx mqtt events 192.168.1.100 --add "tnsaxis:CameraApplicationPlatform/ObjectAnalytics/Device1Scenario1" -u admin -p secret
vapx mqtt events 192.168.1.100 --add "<topic>" --qos 0 --retain none -u admin -p secret
vapx mqtt events 192.168.1.100 --remove "<topic>" -u admin -p secret
vapx mqtt events 192.168.1.100 --clear -u admin -p secret

# Stream status
vapx streamstatus 192.168.1.100 -u admin -p secret

# Device self-test (requires preview mode)
vapx selftest 192.168.1.100 -u admin -p secret

# Signed video
vapx signedvideo status 192.168.1.100 -u admin -p secret
vapx signedvideo enable 192.168.1.100 -u admin -p secret

# ZipStream compression
vapx zipstream status 192.168.1.100 -u admin -p secret
vapx zipstream set 192.168.1.100 --profile classic --level 1 -u admin -p secret

# View areas
vapx viewarea list 192.168.1.100 -u admin -p secret
vapx viewarea get 192.168.1.100 --id 1000001 -u admin -p secret

# --- enroll: factory-default camera setup ---
# Verified against AXIS OS 12.11. See the firmware note below before using
# this on a device older than AXIS OS 11.5.

# Enroll a factory-default camera: creates the initial account, verifies the
# login, derives a name from model and serial, and writes cameras.yaml.
# The password is generated to match the device's own passphrase policy.
vapx enroll 192.168.0.90
vapx enroll 192.168.0.90 --name entrance --to-group site-a
vapx enroll 192.168.0.90 --dry-run            # plan only, touches nothing
vapx enroll 192.168.0.90 --out creds.json     # full result to a 0600 file

# The password is masked in the output by default:
#   "password": "@6… (20 chars)"
# Use --reveal to print it, or --out to write it to a file. If the config
# write fails the credentials are still reported, so they are never lost.

# Readiness and factory-default state — the one endpoint that needs no login.
# needsetup=true means the camera has no account yet and can be enrolled.
vapx systemready 192.168.0.90
vapx systemready mycam --until-ready 240      # poll while the camera boots

# Reboot (also `vapx fw reboot`): sent once, --wait reports the time until the camera is back.
vapx reboot mycam --wait

# After a reboot or factory default, wait for the *new* boot: a camera that
# has been told to restart keeps answering "ready" until it actually goes down.
BOOT=$(vapx systemready mycam | jq -r .data.bootid)
vapx fw factory-default mycam --mode soft --yes   # --yes skips the confirmation
vapx systemready mycam --until-ready 300 --after-bootid "$BOOT"

# Rename a camera, updating its group memberships too. The name enroll
# derives is provisional; real names describe where the camera is.
vapx config rename p1447-le-000006 entrance

# Group membership — groups are what `vapx batch` runs against
vapx config group list
vapx config group add work p1447
vapx config group remove work p1447

# Decommission a camera: entry, group memberships, its own comments and its
# keyring secret, in one step. Asks for the name back before it writes.
vapx config remove keybox          # aliases: delete, rm
vapx config remove keybox -y       # no prompt (required when not a TTY)
vapx config remove keybox --keep-secret

# Show config file location
vapx config path

# Validate config
vapx config check

# List configured cameras
vapx config list

# Create template config
vapx config init

# Change password
vapx pass 192.168.1.100 -u root -p oldpass --name root --pwd newpass

# Network configuration
vapx net show 192.168.1.100 -u admin -p secret
vapx net set 192.168.1.100 root.Network.HostName=myaxis -u admin -p secret

# Time/NTP configuration
vapx time show 192.168.1.100 -u admin -p secret
vapx time set 192.168.1.100 --ntp-server pool.ntp.org -u admin -p secret

# I/O port management
vapx hw show 192.168.1.100 -u admin -p secret
vapx hw trigger 192.168.1.100 --index 0 --state active -u admin -p secret
vapx hw trigger 192.168.1.100 --index 0 --state inactive -u admin -p secret
vapx hw trigger 192.168.1.100 --index 0 --state active --pulse 500 -u admin -p secret

# Stream real-time events via WebSocket
vapx events 192.168.1.100 -u admin -p secret

# Run a command on multiple cameras in parallel
vapx batch info cam1 cam2 cam3 -u admin -p secret
vapx batch fw building_a -u admin -p secret

# Discover supported APIs
vapx discover 192.168.1.100 -u admin -p secret

# Compare parameters between two cameras
vapx diff 192.168.1.100 192.168.1.101 -u admin -p <password>
vapx diff 192.168.1.100 --group-diff building_a -u admin -p secret

# Backup/restore parameters
vapx backup save 192.168.1.100 -u admin -p secret -o backup.json
vapx backup restore 192.168.1.100 -u admin -p secret -i backup.json --dry-run

# Overlay management
vapx overlay list 192.168.1.100 -u admin -p secret

# View system/access logs
vapx log system 192.168.1.100 -u admin -p secret
vapx log access 192.168.1.100 -u admin -p secret

# Generate stream URLs
vapx stream rtsp 192.168.1.100
vapx stream mjpeg 192.168.1.100 --resolution 1920x1080

# Parameter templates (desired-state config)
vapx template create 192.168.1.100 --groups root.Network,root.Time -u admin -p secret -o template.json
vapx template apply 192.168.1.100 -u admin -p secret -i template.json --dry-run
vapx template diff 192.168.1.100 -u admin -p secret -i template.json

# Security audit
vapx audit 192.168.1.100 -u admin -p secret
vapx audit 192.168.1.100 -u admin -p secret --plain

# Certificate management
vapx cert list 192.168.1.100 -u admin -p secret

# Watch events from multiple cameras
vapx watch cam1 cam2 cam3 -u admin -p secret

# Action rule management
vapx rule list 192.168.1.100 -u admin -p secret
vapx rule templates 192.168.1.100 -u admin -p secret

# Storage/SD card management
vapx storage list 192.168.1.100 -u admin -p secret
vapx storage recordings 192.168.1.100 -u admin -p secret
vapx storage health 192.168.1.100 -u admin -p secret

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

## SSH: firmware support

`vapx ssh` uses two separate APIs. The service switch is the parameter
`root.Network.SSH.Enabled`, which every tested release has. Accounts go through
the Device Configuration API: `config/rest/ssh/v2/users` from AXIS OS 12, and
the `v1` beta at `config/rest/ssh/v1beta/users` on 11.5 to 11.x (`v2` is
preferred when a device has both). `vapx ssh status` shows what a device offers
(`account_api`) and how logins work there (`login`):

| `login` | AXIS OS | Who can log in over SSH |
|---------|---------|-------------------------|
| `root` | before 11.5 | Only `root`, with its password. No separate SSH accounts. |
| `root+accounts` | 11.5 to 11.x | `root`, and SSH accounts created with `vapx ssh users`. |
| `accounts` | 12.0 and later | Only SSH accounts; root SSH login is gone. |

| AXIS OS | `status` / `enable` / `disable` | `users` | Tested on |
|---------|----------------------------------|---------|-----------|
| 12.11, 13.1 | Yes | Yes (`v2`) | Q1726, M3128-LVE, M2035-LE, Q3556-LVE |
| 11.11 | Yes | Yes, through `v1` (beta); not yet tried on a device | P3245-LV (status only) |
| 10.12, 8.40 | Yes | No, no account API | M1137, Q6154-E |
| 9.80 | `status` only (switch not tried) | No, no account API | M3045-V, C Cube |

On a device without an account API the `users` commands stop with a clear
message. There, SSH logs in as root: `vapx ssh enable`, then `ssh root@<host>`
with the root password.

If the parameter is rejected, the service switch falls back to `ssh.cgi`
(`getSshInfo` / `setSshConfiguration`). Axis documents `ssh.cgi` as the
successor to the parameter, so older firmware is not where the fallback
applies; no tested release has needed it, and it is verified only against a
mock.

## Enroll: firmware support

`vapx enroll` has been verified end-to-end on both sides of the 11.5 boundary,
each time by factory-defaulting a camera and enrolling it from scratch:

| AXIS OS | Rules for the initial account (per Axis docs) | Status |
|---------|-----------------------------------------------|--------|
| 11.5 and later | Must be Administrator with PTZ. Name is free. | **Verified** on 12.11 (M1137 Mk II) and 11.11 (P1447-LE) |
| Older than 11.5 | Must be named `root`, Administrator with PTZ, and `comment` must be empty or omitted. Can be created once and cannot be deleted. | **Verified** on 10.12 (P1447-LE) |

The oldest tested release is 10.12.338. Below AXIS OS 9.50 enroll cannot work
at all: `systemready.cgi` does not exist there, so there is no way to detect
that a camera needs setup.

Two things worth knowing when testing this yourself:

- **Downgrading requires a hard factory default.** A camera on 11.11 refuses a
  downgrade with `VAPIX error 409: Downgrade is only allowed if
  factoryDefault=hard is set`. `--factory-default hard` also resets the network
  settings, so confirm the device will come back on DHCP before doing it
  remotely.
- **`vapx user list` narrows its output on AXIS OS 10.x and older**, which
  answer with all ~169 system groups rather than the five role groups newer
  firmware reports. Only the role groups (admin, operator, viewer, ptz, users,
  digusers) are shown, with a note on stderr saying how many were hidden; pass
  `--all` for the raw listing.

This is why `--account` defaults to `root`: it is the only name accepted
across every generation, and the firmware version cannot be read beforehand —
a factory-default camera answers 401 on `basicdeviceinfo.cgi` and `param.cgi`.
`--role` always includes PTZ for the same reason.

If you enroll an older device, `vapx systemready <host>` first will at least
confirm `needsetup` and the passphrase policy, both of which work without
credentials on AXIS OS 9.50 and later.

## MCP server

`vapx mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server built into the CLI, so an AI assistant can look at your cameras with the same code and the same `cameras.yaml` the CLI uses. It has the tool names, parameters and descriptions of the Python [vapx-mcp](https://github.com/Hex29A/vapx-mcp) server (78 tools), so existing clients keep working. Cameras are identified by their key in `cameras.yaml` (`camera_id`); the file is read on every call, so changes made with `vapx config` apply without a restart.

### Tools

93 tools: the 78 of the Python server, plus 15 for features of the CLI it never had (firmware, parameters, imaging, fleet health, audits, backups, diffs, certificates, stream URLs, self test, ZipStream). "read" tools only look; "write" tools change a camera, run something on it, or write a file on the machine running the server.

| Tool | Access | What it does |
|---|---|---|
| `activate_siren` | write | Activate a siren and/or strobe light on a deterrence device. |
| `add_overlay` | write | Add a text overlay to the camera's live video stream. |
| `add_privacy_mask` | write | Add a privacy mask to cover a sensitive area in the video. |
| `backup_params` | write | Save a camera's parameters as a JSON file on the machine running this server (they can contain secrets, so the file is readable by its owner only). |
| `cert_list` | read | List the certificates installed on a camera. |
| `check_systemready` | read | Check if an Axis camera is ready to handle requests. |
| `configure_mqtt` | write | Configure the MQTT client connection on a camera. |
| `create_stream_profile` | write | Create a new video stream profile with specified parameters. |
| `delete_audio_clip` | write | Delete an audio clip from the camera. |
| `diff_params` | read | Compare the parameters of two cameras and list what differs. |
| `disable_mqtt` | write | Disable (deactivate) the MQTT client on a camera — disconnects from the broker. |
| `discover_apis` | read | List all VAPIX APIs supported by an Axis device. |
| `enable_mqtt` | write | Enable (activate) the MQTT client on a camera — connects to the configured broker. |
| `export_recording` | write | Export a recording from the camera's edge storage as a .mkv file. |
| `firmware_check` | read | Check the camera's firmware version against an expected version, or against another camera. |
| `firmware_status` | read | Show the camera's firmware status: active and inactive versions, build date and whether the firmware is committed. |
| `firmware_upgrade` | write | Upload a firmware .bin file from the server's disk and install it. |
| `fleet_health` | read | Check the health of several cameras in parallel: reachability, model, firmware, latency and issues. |
| `get_audio_settings` | read | Get audio device settings — input/output configuration, gain levels, mute status, and connection types. |
| `get_audit_log` | read | Read the security audit log from an Axis camera. |
| `get_camera_info` | read | Get device information for an Axis camera: model, serial number, firmware version, hardware ID, architecture, etc. |
| `get_capture_modes` | read | List available video capture modes (resolution + max FPS) for a camera. |
| `get_clear_view_info` | read | Get Clear View service info — available cleaning services (wiper, speed-dry), duration limits, and cooldown times. |
| `get_daynight_config` | read | Get the day/night (IR-cut filter) configuration for a camera — shift levels, dwell times, auto-tune, and IR-pass filter settings. |
| `get_disk_status` | read | Get storage disk status for a camera — SD card and network share info including total size, free space, health, filesystem, and lock status. |
| `get_geolocation` | read | Get the camera's configured GPS coordinates (latitude, longitude), compass heading, and location description text. |
| `get_io_ports` | read | List all I/O ports on a camera/device with their current states, directions (input/output), and configurations. |
| `get_lights` | read | List all lights (IR, white, indicator) on a camera with their current state, type, and configuration. |
| `get_motion_config` | read | Get the current video motion detection (VMD4) configuration, including detection zones, filters, and trigger areas. |
| `get_mqtt_config` | read | Get MQTT client configuration and connection status — broker address, port, client ID, connection state, and event publication settings. |
| `get_ntp_status` | read | Get NTP time synchronization status — whether NTP is enabled, sync status, configured servers, time offset, and next sync time. |
| `get_orientation` | read | Read the camera's physical orientation from its built-in sensor (accelerometer/gyroscope). |
| `get_recording_info` | read | Get export properties for a specific recording — estimated file size, exact start/stop times, export format. |
| `get_signed_video_status` | read | Check whether signed video is enabled on a camera. |
| `get_siren_status` | read | Get the current status of siren and strobe light devices. |
| `get_snapshot` | read | Capture a still JPEG image from an Axis camera. |
| `get_stream_status` | read | Get real-time stream diagnostics for a camera — active client count, bitrate (kbps), FPS, resolution, and codec for each stream. |
| `get_system_log` | read | Read the system log from an Axis camera. |
| `get_temperature` | read | Read temperature sensors (CPU, main board, lens, etc.) and heater status from a camera. |
| `get_time_info` | read | Get the camera's current date/time, timezone, and DST status. |
| `get_users` | read | List all user accounts on a camera with their group memberships (admin, operator, viewer, ptz). |
| `imaging_get` | read | Read the image sensor settings (brightness, contrast, exposure, WDR and others) as parameters. |
| `imaging_set` | write | Change image sensor settings. |
| `list_analytics_producers` | read | List analytics metadata producers (object detection, motion, etc.) and whether they are enabled per video channel. |
| `list_applications` | read | List all installed ACAP applications on a camera with their status (Running/Stopped/Idle), license validity, vendor, version, and resources used. |
| `list_audio_clips` | read | List audio clips stored on a camera. |
| `list_cameras` | read | List all configured Axis cameras with their IDs, names, and capabilities |
| `list_guard_tours` | read | List all configured guard tours (automated PTZ patrol routes) on a camera, including their presets and running status. |
| `list_overlays` | read | List all text and image overlays currently on the camera's video stream. |
| `list_privacy_masks` | read | List all privacy masks on a camera with their pixel coordinates, names, enabled status, and zoom visibility. |
| `list_recordings` | read | List recordings stored on the camera's edge storage (SD card or network share). |
| `list_stream_profiles` | read | List video stream profiles — preset configurations for resolution, codec, FPS, and other streaming parameters. |
| `list_view_areas` | read | List all view areas (virtual channels) on a camera. |
| `param_get` | read | Read camera parameters by key or group, e.g. |
| `param_set` | write | Set camera parameters by explicit key. |
| `play_audio_clip` | write | Trigger playback of an audio clip on the camera's built-in speaker. |
| `poll_events` | write | Poll for camera events over WebSocket for a specified duration. |
| `ptz_home` | write | Return a PTZ camera to its home position. |
| `ptz_move` | write | Move a PTZ camera to an absolute pan/tilt/zoom position. |
| `ptz_preset` | write | Move a PTZ camera to a named preset position. |
| `ptz_relative` | write | Move a PTZ camera by a relative offset from its current position. |
| `ptz_status` | read | Get the current pan/tilt/zoom position of a PTZ camera. |
| `reboot_camera` | write | Reboot an Axis camera. |
| `remove_overlay` | write | Remove an overlay from the camera's video stream by its identity. |
| `remove_privacy_mask` | write | Remove a privacy mask by name. |
| `remove_stream_profile` | write | Remove a video stream profile by name. |
| `reset_view_area_geometry` | write | Reset a view area's geometry back to its factory default (full canvas). |
| `restart_application` | write | Restart an ACAP application on a camera. |
| `security_audit` | read | Audit the security posture of one camera, or of every configured camera when camera_id is left out: unencrypted HTTP, old firmware, default user, S... |
| `selftest` | write | Run the camera's built-in self test and return its result. |
| `set_analytics_producers` | write | Enable or disable specific analytics metadata producers per video channel. |
| `set_audio_settings` | write | Update audio device settings — change gain, mute, or input source. |
| `set_capture_mode` | write | Switch a camera to a different capture mode (resolution/FPS). |
| `set_daynight_config` | write | Configure day/night switching behavior — thresholds for switching between day mode (color) and night mode (IR/B&W). |
| `set_geolocation` | write | Set the camera's GPS coordinates and heading. |
| `set_io_port` | write | Set the state of a digital output port. |
| `set_motion_config` | write | Set video motion detection (VMD4) configuration. |
| `set_ntp_config` | write | Configure NTP time synchronization — enable/disable, set server source (static or DHCP), and configure NTP server addresses. |
| `set_timezone` | write | Set the camera's timezone using an IANA timezone name (e.g. |
| `set_view_area_geometry` | write | Set the crop geometry for a view area, digitally repositioning where the camera 'looks' without moving the physical lens. |
| `snapshot_all` | write | Capture a JPEG snapshot from every configured camera simultaneously. |
| `start_application` | write | Start a stopped ACAP application on a camera. |
| `start_clear_view` | write | Activate the camera's wiper or speed-dry function to clean the lens. |
| `start_guard_tour` | write | Start a guard tour (automated PTZ patrol). |
| `status_all` | read | Get a quick status summary of all configured cameras — model, firmware, online/offline, and key capabilities. |
| `stop_application` | write | Stop a running ACAP application on a camera. |
| `stop_audio_clip` | write | Stop any currently playing audio clip on the camera. |
| `stop_clear_view` | write | Stop a running wiper/speed-dry operation (if stoppable). |
| `stop_guard_tour` | write | Stop a running guard tour. |
| `stop_siren` | write | Stop all active sirens and lights on a deterrence device. |
| `stream_urls` | read | Build RTSP, MJPEG and snapshot URLs for a camera. |
| `toggle_light` | write | Turn a camera light on or off. |
| `zipstream_status` | read | Show the camera's ZipStream compression profiles and maximum level. |

- Every tool is marked read-only or not in its annotations. The ones that interrupt a camera or cannot be undone (reboot, firmware upgrade, siren, relay outputs, removals, application restarts, parameter changes, ...) are marked destructive and say so in their description, so a client can ask before calling them.
- Arguments are checked against each tool's input schema before anything is sent to a camera.
- `reboot_camera` sends the request once: a connection that closes while the camera goes down counts as sent.
- `firmware_upgrade` uploads a `.bin` file from the server's disk and reboots the camera. It only runs with `confirm: true`, never factory-defaults, and commits only with `auto_commit` (which needs `wait`).
- `param_set` takes explicit keys only (no wildcards or groups, at most 50) and refuses keys under `root.Network`, `root.Users` and `root.Properties`.
- `param_get` and `diff_params` show the values of secrets (passwords, tokens, community strings) as `[redacted]`. `backup_params` keeps them, which is why it saves a file readable only by its owner instead of returning the parameters.
- `export_recording` and `backup_params` write under `$VAPX_MCP_EXPORTS_DIR` (the Python server's `$VAPIX_EXPORTS_DIR` also works), by default `vapx/exports` in your user data directory, and only accept plain file names.
- `stream_urls` returns URLs without credentials.
- `add_privacy_mask` takes the polygon as it appears in a snapshot; set `mask_rotation` and `mask_mirror` for the camera in `cameras.yaml` if its image is rotated or mirrored.

**Not available as tools**, because they can lock you out of a camera: factory default, changing passwords, creating or deleting users, SSH accounts, network settings and enrolling a camera. Use the command line for those.

`list_cameras` returns an empty `capabilities` list: the server does not probe cameras to list them, and a tool the camera does not support returns the camera's own error.

### Client configuration: stdio

The client starts the server itself. For Claude Desktop, Claude Code or any client that takes a command:

```json
{
  "mcpServers": {
    "vapx": { "command": "vapx", "args": ["mcp"] }
  }
}
```

Add `"--config", "/path/to/cameras.yaml"` to `args` to use a specific file (otherwise `$VAPX_CONFIG` or `~/.config/vapx/cameras.yaml`).

### Client configuration: HTTP

```bash
# Paste a long random string (for example from `openssl rand -hex 32`) when asked.
read -rs VAPX_MCP_TOKEN && export VAPX_MCP_TOKEN
vapx mcp --transport http --bind 127.0.0.1:8080
```

The endpoint is `http://127.0.0.1:8080/mcp` (Streamable HTTP, no SSE). Every request needs `Authorization: Bearer <token>`:

```json
{
  "mcpServers": {
    "vapx": {
      "type": "http",
      "url": "http://127.0.0.1:8080/mcp",
      "headers": { "Authorization": "Bearer <token>" }
    }
  }
}
```

- The server refuses to start without `VAPX_MCP_TOKEN`; `--insecure-no-auth` switches authentication off and is only for a trusted loopback setup. Tokens are compared in constant time.
- `--bind` is required: there is no default address, and nothing listens on all interfaces unless you pass `0.0.0.0` yourself.
- Requests are accepted for the bind address, `localhost` and `127.0.0.1`. If clients reach the server by another name, add it with `--allowed-host <name>` (repeatable).
- The connection is plain HTTP. Put a TLS-terminating proxy in front, or keep it on loopback or a private network.

### Container

The `Dockerfile` builds an image that runs `vapx mcp --transport http` as a non-root user. The camera list comes from a directory mounted at `/config` (`VAPX_CONFIG` points at `/config/cameras.yaml`; mount the directory, not the file, because vapx replaces `cameras.yaml` atomically), and `export_recording` and `backup_params` write to `/exports`.

```bash
docker build -t vapx-mcp .

mkdir -p exports
read -rs VAPX_MCP_TOKEN && export VAPX_MCP_TOKEN   # paste a long random string

docker run -d --name vapx-mcp --restart unless-stopped \
  --user "$(id -u):$(id -g)" \
  -p 127.0.0.1:8080:8080 \
  -v "$HOME/.config/vapx:/config:ro" \
  -v "$PWD/exports:/exports" \
  -e VAPX_MCP_TOKEN \
  vapx-mcp
```

- `--user` makes the container read `cameras.yaml` as you, which matters when the file is readable by its owner only. Without it the container runs as user 10001.
- The server listens on all interfaces *inside* the container; the example publishes the port on the host's loopback only. Publish on another address deliberately, and put TLS in front of it.
- If clients reach the server by a name other than an IP address or `localhost`, add it with `-e VAPX_MCP_ALLOWED_HOSTS=mcp.example.org` (comma-separated).
- Check it: `curl -s -X POST http://127.0.0.1:8080/mcp -H "Authorization: Bearer $VAPX_MCP_TOKEN" -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'` lists the tools; without the header the answer is `401`.

### Migrating from the Python vapx-mcp

The tool names, parameters and descriptions are the same, and so is `cameras.yaml` (the camera ID is the key in the file), so a client only needs a new server entry.

| | Python vapx-mcp | `vapx mcp` |
|---|---|---|
| Start (stdio) | `python server.py` | `vapx mcp` |
| Start (HTTP) | `python server.py --transport streamable-http --port 8080` | `vapx mcp --transport http --bind 127.0.0.1:8080` |
| HTTP endpoint | `/mcp` (and `/sse`) | `/mcp` |
| SSE transport | deprecated, still there | gone: use Streamable HTTP |
| Token | `VAPX_MCP_TOKEN` for HTTP | the same; the server refuses to start without it |
| Config file | `VAPIX_CONFIG` | `--config`, `VAPX_CONFIG`, or `VAPIX_CONFIG` as before |
| Exports directory | `VAPIX_EXPORTS_DIR` (default `/exports`) | `VAPX_MCP_EXPORTS_DIR` (or `VAPIX_EXPORTS_DIR`); default `vapx/exports` in the user data directory, `/exports` in the container image |
| Bind address | `--port`, all interfaces | `--bind <addr:port>`, required |

Client configuration for stdio is `{"command": "vapx", "args": ["mcp"]}` instead of the `docker run` or `python` command; for HTTP, point the client at `http://<host>:<port>/mcp` with the same `Authorization: Bearer` header.

What differs in behaviour:

- `list_cameras` has an empty `capabilities` list and the server does not check capabilities before a call; an unsupported tool returns the camera's own error.
- Some results follow the CLI's shapes instead of the Python server's (for example `get_temperature`, `list_recordings`, `get_disk_status`, `get_stream_status`).
- Failed calls are marked `isError` (the text is still `Error: ...`).
- Tools check their arguments against the input schema first, and `reboot_camera` is sent once without retries.
- `export_recording` accepts plain file names only.
- 15 tools are new (see the table above), and log output goes to stderr.

## Configuration

Config file search order:

1. `--config <path>` flag
2. `$VAPX_CONFIG` environment variable
3. `~/.config/vapx/cameras.yaml` (XDG config directory)

There is no lookup in the current working directory — use `--config` or
`VAPX_CONFIG` to point at a config elsewhere.

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
| `mask_rotation` | int | `0` | Image rotation in degrees (0, 90, 180, 270). `vapx mcp` uses it to convert privacy mask polygons from the displayed image to the sensor |
| `mask_mirror` | bool | `false` | Whether the image is mirrored (see `mask_rotation`) |

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

1. `cameras.yaml` lookup (by camera name or host). `-u`/`-p` override the
   stored username and password, but the entry still supplies the host,
   port, and TLS settings.
2. OS keyring lookup, for a camera in the config with no `pass` (requires
   `--features keyring`; the published release binaries do not include it)
3. `-u`/`-p` flags, for a host that is not in the config at all
4. Interactive prompt (TTY only)

The config is consulted first so that a configured camera *name* always
resolves to its real address, even when credentials are given on the command
line. Before v0.22.0 the flags short-circuited this, and `vapx info mycam -u
x -p y` tried to resolve "mycam" as a DNS hostname.

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
# Unit tests (no camera needed) — this is what CI runs
cargo test --bin vapx

# Full integration tests (requires a reachable camera)
VAPX_TEST_HOST=192.168.1.100 VAPX_TEST_USER=root VAPX_TEST_PASS=secret cargo test

# Integration tests skip gracefully if the camera is unreachable
cargo test --test integration
```

`cargo test --lib` does not work: vapx is a single binary crate with no
library target.

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
    ssh.rs             # vapx ssh — SSH service and SSH accounts
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
    enroll.rs          # vapx enroll — factory-default camera setup
    systemready.rs     # vapx systemready — readiness / needs-setup state
  vapix/
    auth.rs            # Digest/Basic auth negotiation
    client.rs          # VapixClient (HTTP with auth, retry, error sanitization)
    device.rs          # basicdeviceinfo.cgi
    firmware.rs        # firmwaremanagement.cgi
    applications.rs    # ACAP list/control (XML parsing)
    ptz.rs             # PTZ control (com/ptz.cgi)
    params.rs          # Parameter management (param.cgi)
    users.rs           # User management (pwdgrp.cgi)
    ssh.rs             # SSH switch (param.cgi) and SSH accounts (config/rest/ssh/v2)
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
    systemready.rs     # Systemready API (unauthenticated readiness probe)
  enroll/
    naming.rs          # Config-key derivation from model + serial
    password.rs        # Policy-aware password generation
  config/
    cameras.rs         # cameras.yaml loading, env substitution, name resolution
    credentials.rs     # Credential resolution (yaml > keyring > flags > prompt)
    writer.rs          # Safe cameras.yaml edits (validated, atomic, comment-preserving)
  output/
    format.rs          # JSON, table, CSV, YAML formatters
tests/
  integration.rs       # Live camera integration tests (76 tests)
```

## VAPIX API Coverage

- [x] Basic Device Information (`basicdeviceinfo.cgi`)
- [x] JPEG Snapshot (`jpg/image.cgi`)
- [x] Firmware Management (`firmwaremanagement.cgi`)
- [x] ACAP Application Lifecycle (`applications/*.cgi`)
- [x] PTZ Control (`com/ptz.cgi`)
- [x] Parameter Management (`param.cgi`)
- [x] User Management (`pwdgrp.cgi`)
- [x] SSH service and accounts (`param.cgi root.Network.SSH`, `config/rest/ssh/v2/users`)
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
- [x] Audio Clip Management (`mediaclip.cgi`)
- [x] Systemready (`systemready.cgi`) — unauthenticated readiness and needs-setup state

See [`docs/API.md`](docs/API.md) for the full command-to-endpoint mapping.

## License

MIT
