# Changelog

## v0.15.0

### Added
- **`clip`**: New command for audio clip management on cameras with audio output. Subcommands: `list` (show stored clips), `play` (trigger playback on built-in speaker), `upload` (send a .wav file to the camera via multipart), `delete` (remove a clip by name). Accepts clip names with or without the `clip:` prefix. Gracefully reports errors from cameras that lack audio clip support (older firmware, no speaker hardware). New VAPIX module: `vapix/audio_clip.rs`. New client helper: `post_multipart_file()` in `VapixClient`.

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
