# Changelog

## v0.18.0

### Fixed
- **Firmware upload: preemptive auth**: Digest authentication now probes with an empty request before sending the firmware body, eliminating the double-upload that caused 50 MB uploads to take ~128s and timeout. Upload time is halved (closes #33).

### Added
- **`fw_timeout` config field**: Per-camera `fw_timeout` in `cameras.yaml` overrides the general `timeout` for firmware operations. Default firmware timeout increased from 120s to 300s (closes #34).
- **Firmware upload progress bar**: `vapx fw upgrade` now shows a byte-progress bar with transfer speed instead of a spinner during upload (closes #35).
- **`--auto-commit` flag**: `vapx fw upgrade --wait --auto-commit` automatically commits firmware after the camera reboots successfully, eliminating the manual `vapx fw commit` step. Requires `--wait` (closes #36).

## v0.17.2

### Added
- **`enabled` flag**: Cameras in `cameras.yaml` now support `enabled: false` to skip them in `batch`/`watch`/`health` without removing them from the config. Defaults to `true` for backward compatibility (closes #31).
- **Config reference table**: README now includes a complete field reference table for `cameras.yaml` (`defaults`, per-camera fields, `profiles`, `groups`). The `vapx config init` template has been expanded with all fields commented out and annotated (closes #32).

## v0.17.1

### Fixed
- **`stream nexus` removed**: The `vapx stream nexus` subcommand has been removed. Device Data Hub (`ws-data-stream`) is a pub/sub data system, not a video streaming endpoint — the URL always returned HTTP 400 (closes #29).
- **`selftest` test**: Fixed integration test to handle tracing output (WARN lines) that could pollute stdout and cause JSON parse failures.

### Changed
- **Shared helpers**: Extracted `resolve_cam` and `make_client` into `cmd/mod.rs`, replacing 8 duplicate implementations across storage, cert, rule, fw, overlay, clip, mqtt, and signedvideo modules (closes #27).
- **Shared `resolve_targets`**: Extracted duplicated group/camera resolution logic from batch, watch, and health into `cmd/mod.rs` (closes #28).
- **Discover hints**: All "not available on this camera" error messages now suggest `vapx discover` to check supported APIs (closes #30).

## v0.17.0

### Added
- **`stream nexus`**: Generate a Nexus (WebSocket video) stream URL — `vapx stream nexus <camera>` returns `ws://…/vapix/ws-data-stream?wssession=<token>&sources=video` with a fresh session token (closes #26). Use `--no-fetch-token` to emit a URL template with a `<TOKEN>` placeholder (no network call) for scripting and documentation. HTTPS cameras emit `wss://` automatically.

### Fixed
- **`streamstatus`**: No longer presents `param.cgi` stream-config parameters as if they were live statistics (fixes #25). The command now:
  1. tries `streamstatus.cgi` over HTTP,
  2. then `/vapix/ws-data-stream?sources=streamstatus` over WebSocket,
  3. finally falls back to `param.cgi root.Image.I0.Stream.*` with an explicit `note` clarifying that `0` means *unlimited*, not *no active streams*, and a `source: "param_cgi_fallback"` marker.
  Every response now includes a `source` field identifying which path produced the data.

### Changed
- Extracted shared WebSocket helpers (`get_ws_session`, `build_ws_url`, `build_nexus_url`) into `src/vapix/ws.rs`. `vapix events` now uses this helper.

## v0.16.2

### Fixed
- **`storage health` / `storage info`**: Fall back to `disks/list.cgi` data when `disks/properties.cgi` returns 404 (fixes #23). Cameras running AXIS OS 12.x (e.g. M3128-LVE) that don't support `properties.cgi` now return disk health data from `list.cgi` instead of errors.
- **`signedvideo status` / `enable` / `disable`**: Fall back to `param.cgi` (`root.SignedVideo` / `root.Properties.API.SignedVideo`) when `signedvideo.cgi` returns 404 (fixes #24). Cameras where the CGI endpoint is absent but signed video parameters exist now return status via param.cgi.

## v0.16.1

### Fixed
- **`storage recordings`**: Add `maxnumberofrecordings=1000` parameter so all recordings are returned, not just one. New `--max <N>` flag to control the limit.

### Changed
- **`info`**: Replace useless `WebURL` (always "https://www.axis.com") with `DeviceURL` showing the actual camera URL (e.g. `"http://192.168.7.10"`).

## v0.16.0

### Fixed
- **`clip`**: Rewrote to use the correct VAPIX Media Clip API.

### Changed
- **`clip list`**: Now reads clips from `param.cgi?group=MediaClip`.
- **`clip play` / `clip delete`**: Accept either a clip name or integer ID.
- **`clip upload`**: Field name is now the clip display name. Accepts `--name` to override.

### Added
- **`clip stop`**: New subcommand to stop any currently playing clip.

## v0.15.0

### Added
- **`clip`**: Audio clip management (list, play, upload, delete). New VAPIX module: `vapix/audio_clip.rs`.

## v0.14.0

### Added
- **`hw trigger`**: New subcommand to activate/deactivate I/O output ports via `io/port.cgi`. Supports `--state active|inactive|on|off` and `--pulse <ms>` for timed pulses. Port must be configured as output (`hw set --direction output`) before triggering.

## v0.13.1

### Fixed
- **HTML in error messages**: APIs returning 404 no longer dump raw HTML pages into JSON error output. The `VapixClient` now extracts the `<title>` text from HTML responses for clean error messages. Affects `viewarea`, `zipstream`, `signedvideo`, `storage health`, and any other command hitting unsupported APIs.
- **`ptz info` error handling**: Returns proper JSON error envelope when PTZ is disabled instead of raw text to stdout.
- **`zipstream` error detection**: Made case-insensitive ("Not Found" vs "not found") so error responses are correctly caught regardless of server response casing.

## v0.13.0

### Changed
- **`acap start/stop/restart/remove`**: Package name is now a positional argument instead of `--package` flag.
- **`ptz preset --save`**: New `--save` flag to save current position as a named preset.
- **`ptz query`**: Added `attributes` and `auxiliary` query types.

### Fixed
- **`storage list`**: Recordings now parsed via XML (roxmltree) instead of fragile text parsing.
- **`storage health`**: Added disk health subcommand for disk properties.
- **`hw show`**: Falls back to legacy `param.cgi` when `portmanagement.cgi` is unavailable.

## v0.12.0

### Added
- `streamstatus` — stream status and parameters
- `selftest` — device self-test (preview mode)
- `signedvideo` — signed video management (status, enable, disable)
- `zipstream` — ZipStream compression profiles (status, set)
- `viewarea` — view area management (list, get, set geometry)
- `mqtt` — MQTT client management (status, configure, enable, disable, events)

## v0.11.0

### Added
- `temp` — temperature sensor readings
- `daynight` — IR-cut filter mode
- `imaging` — image sensor settings (brightness, contrast, exposure, WDR)
- `light` — IR illuminator status and intensity
- `vmd` — video motion detection configuration
- `audio` — audio source configuration

## v0.10.1

### Fixed
- `storage list` fallback for cameras without modern disk API
- `fw check` argument handling
- `cert` and `rule` error responses
- `config path` XDG resolution

## v0.10.0

### Added
- `rule` — action rule management (list, info, enable, disable, remove, templates)
- `storage` — SD card and edge storage management (list, recordings, params)
- `health` — fleet health check (parallel, model/firmware/latency/issues)

## v0.9.0

### Added
- `log` — system/access log viewer
- `stream` — RTSP/MJPEG/snapshot URL builder
- `template` — desired-state parameter templates (create, apply, diff)
- `audit` — security posture audit
- `cert` — certificate management (list, self-sign, CSR, remove)
- `watch` — multi-camera event monitoring (threaded)
- `fw check` — firmware version comparison
- `diff --group-diff` — diff reference camera against entire group
- Output formats: `--format table|csv|yaml`
- Time-lapse snapshots: `snap --interval/--count`

## v0.8.0

### Added
- `discover` — API discovery
- `diff` — parameter diff between two cameras
- `backup` — parameter backup/restore
- `overlay` — text/image overlay management
- Progress bars for batch and firmware operations
- `--filter` flag for extracting specific JSON fields
- Config profiles (`--profile`)
- OS keyring secrets backend (`--features keyring`)

## v0.7.0

### Added
- `fw upgrade/commit/rollback/reboot/factory-default` — full firmware lifecycle
- `events` — real-time event streaming via WebSocket

## v0.6.0

### Added
- `batch` — run command on multiple cameras in parallel
- `mangen` — man page generation
- `config add` — add camera with connectivity verification

## v0.5.0

### Added
- `time` — NTP/timezone management
- `hw` — I/O port management
- `completions` — shell completions (bash, zsh, fish)

## v0.4.0

### Changed
- All commands output JSON envelope format (`{"status":"ok","data":{...}}`) for agent-friendly consumption.

## v0.3.0

### Added
- `pass` — password management
- `net` — network configuration (show, set)

## v0.2.0

### Added
- `ptz` — PTZ control (move, goto, preset, query, info)
- `param` — parameter management (list, get, set)
- `user` — user account management (list, add, update, remove)
- Retry with exponential backoff (3 attempts on 5xx/timeout)
- Per-command timeout defaults

## v0.1.0

### Added
- `info` — device identification
- `snap` — JPEG snapshot
- `fw` — firmware status
- `acap` — ACAP application management (list, start, stop, restart, remove)
- `config` — cameras.yaml management (path, check, list, init)
- Digest/Basic auth auto-negotiation
- cameras.yaml with environment variable substitution
- CI pipeline with cross-platform release builds
