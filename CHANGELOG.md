# Changelog

## v0.24.0

### Added
- **`vapx config group list | add | remove`**: manage group membership from the CLI. `add_to_group` already existed but was reachable only through `vapx enroll --to-group`, so putting an existing camera into a group meant hand-editing YAML — the same gap `config rename` closed for names. `add` refuses a camera that is not in `cameras.yaml` (a group of names that resolve to nothing is worse than an error) and a group that does not exist; `remove` is idempotent. Removing the last member writes `group: []` rather than a bare key, which YAML would otherwise read as null and the config would refuse to parse.

## v0.23.0

### Added
- **`vapx config rename <from> <to>`**: rename a camera and update every group that references it, in one step. The name `enroll` derives (`p1447-le-9c57f2`) is always provisional — real names describe where a camera is — and renaming previously meant hand-editing YAML. Same guarantees as the other config writes: validated by re-parsing (camera count, group membership and ordering must be preserved), atomic, backed up, chmod 600, comments intact. A camera whose name merely *contains* the old one is left alone.
- **`vapx user list --all`**, and role-group filtering by default: AXIS OS 10.x and older return all ~169 system groups, which buried the five that describe access. The default output now shows only role groups, with a note on stderr; `--all` restores the full listing. Newer firmware is unaffected.

### Fixed
- **A camera older than AXIS OS 9.50 now says so.** `systemready.cgi` does not exist below 9.50, and the resulting 404 surfaced as "Could not reach \<host\> — HTTP 404", which blames the network for what is really firmware age. The error now names the 9.50 requirement and points at `vapx user add --initial` as the manual path. Genuine connection failures are unchanged.

## v0.22.0

### Added
- **`vapx enroll <host>`**: set up a factory-default camera in one step — detect that it needs setup, generate a password that satisfies the device's own policy, create the initial administrator account, verify the login works, derive a config name from model and serial, and write the entry to `cameras.yaml`. Verified end-to-end against a factory-defaulted M1137 Mk II (AXIS OS 12.11.77).
  - `--name` to choose the config key, otherwise derived as model plus serial tail (`m1137-mk-ii-f09697`), with collision fallback to the full serial.
  - `--to-group` to file the camera in an existing group. Without it the camera joins no group: a group is a target for `vapx batch`, and a freshly enrolled camera is usually still on the bench. A non-existent group is refused *before* the camera is touched, since the account cannot be created twice.
  - `--role` (default `admin`) always includes PTZ — the initial account is documented as requiring Administrator with PTZ on every generation.
  - `--account` (default `root`): AXIS OS older than 11.5 accepts no other name for the first account, and the version cannot be read before that account exists.
  - The password is **masked by default**. Use `--reveal` to print it, or `--out <file>` to write the full result to a 0600 file. If the config write fails, the credentials are still reported — the camera is never left with a password nobody has.
  - `--dry-run` reports the device state, policy and plan without touching camera or config.
  - **Firmware support**: verified end-to-end on both sides of the 11.5 boundary — AXIS OS 12.11 (M1137 Mk II), 11.11 and 10.12 (P1447-LE), each factory-defaulted and enrolled from scratch. The 10.12 run confirms the stricter pre-11.5 rules for the initial account (`root`, Administrator with PTZ, no `comment`) are satisfied. Below AXIS OS 9.50 enroll cannot work at all, since `systemready.cgi` does not exist. See "Enroll: firmware support" in the README.
- **`vapx systemready <host>`**: readiness and factory-default state, over the one VAPIX endpoint that answers **without credentials**. Reports `needsetup` (no initial account yet) and `passphrasepolicy`. `--until-ready` polls while a camera boots; `--after-bootid` waits for a *different* boot, because a camera told to reboot keeps answering "ready" for several seconds before it goes down.
- **`vapx user add --initial`** (primary group `root`, for the first account on a factory-default device) and **`--strict-pwd`** (enforce the VAPIX password standard).

### Fixed
- **`-u`/`-p` bypassed the `cameras.yaml` lookup**: supplying credentials made `credentials::resolve` return before the config was consulted, so a configured camera *name* was resolved as a DNS hostname instead of its host. `vapx info m1137 -u x -p y` failed with a DNS error. The config is now consulted first; explicit flags still override the stored user and password — they override the credentials, not the address.

### Security
- **`pwdgrp.cgi` writes now go in the request body, not the URL.** Creating or updating an account put the password in the query string, where it lands in the camera's access log and in any error message that echoes the URL. Axis documents this explicitly: "It is not advisable to create user access data in the URL." `add` and `update` now POST form-encoded; only the field *names* are logged.

## v0.21.4

### Fixed
- **`vapx config add` wrote the camera into the wrong section and corrupted the config**: the entry was inserted immediately before the `groups:` line, but the template written by `vapx config init` has a `profiles:` section between `cameras:` and `groups:`. The new camera therefore landed inside the `profiles:` block, directly after a flow mapping, and the file stopped parsing — `vapx config check` reported `CONFIG_INVALID` and every subsequent command lost all cameras. Configs without a `profiles:` section happened to escape this.

### Changed
- **Config writes are now safe by construction** (`src/config/writer.rs`): the entry is placed at the end of the `cameras:` block, the edited document is re-parsed *before* it replaces anything (an edit that would not parse, or that would drop an existing camera, is refused and the file is left untouched), the write lands atomically via a temp file in the same directory, the original is kept as `cameras.yaml.bak`, and the config is chmod 600 — it holds passwords in plain text. Comments and camera order are preserved, so hand-maintained notes in the file survive.
- **`config add` warns when no password is given**: such an entry is unusable, and the vapx-mcp server treats it as a hard error for the whole config.

## v0.21.3

### Fixed
- **Storage health/info fell back to a method that omits sizes** (#37): when `disks/properties.cgi` returned 404, both fallback paths used the modern JSON `listDisks`, which drops `totalsize`/`freesize` on some AXIS OS 12.x firmware — so the fields were silently missing. They now use the classic `disks/list.cgi` XML endpoint, which does carry them. Verified against two cameras on AXIS OS 12.11.37.
- **`log report` timed out over WAN links** (#38): a server report runs to several hundred KB and took 30-35s to generate and download, against the general 10s timeout. The default for `Report` is now 120s; `--timeout` still overrides.

## v0.21.2

### Fixed
- **MQTT event publication sent the wrong params shape**: `configureEventPublication` expects `eventFilterList` (and the other config fields) directly under `params`, but v0.21.1 wrapped them in a `params.eventPublicationConfig` object, so cameras responded `VAPIX error 2103: Required parameter missing: params.eventFilterList`. The config object is now passed directly as `params`. Verified end-to-end against a live AXIS camera: `--add` / `--clear` succeed and the filter round-trips through `getEventPublicationConfig`. (Note the API asymmetry: the read method nests the config under `eventPublicationConfig`, the write method does not.)

## v0.21.1

### Fixed
- **MQTT event publication used the wrong VAPIX method**: `vapx mqtt events <host> --add/--remove/--clear` POSTed `method: "setEventPublicationConfig"`, which all cameras reject with `VAPIX error 2102: Method not supported`. The correct method is `configureEventPublication` (same endpoint/params; the read method `getEventPublicationConfig` was already correct and is unchanged).

## v0.20.0

### Changed
- **Centralized `CameraArgs`**: All camera-connection flags (`host`, `-u`, `-p`, `-k`, `--port`, `--plain`, `--timeout`) now come from one shared `clap::Args` struct in `cmd/mod.rs`. As a side effect, `clip`, `viewarea`, `signedvideo`, and `mqtt` now support `--plain` output.
- **Output consolidation**: Introduced `format::output(data, plain)` to replace repeated plain/JSON branches across `storage`, `rule`, `cert`, `overlay`, `fw`, `info`, and `streamstatus`.
- **Parameter parsing reuse**: `storage` and `param list` now share the `param_to_json()` parser.
- **Internal cleanup**: Extracted multipart-body helpers in `client.rs` (deduplicating three upload methods); decomposed the large `fw` and `audit` command handlers into focused functions.
- **Removed unused `thiserror` dependency** (the codebase uses `anyhow` throughout).

### Added
- **`vapx mqtt events` filter management**: `vapx mqtt events <host>` still shows the event publication config; passing `--add <topic>` (repeatable), `--remove <topic>`, and/or `--clear` now performs a read-modify-write that changes only `eventFilterList` (other fields like `topicPrefix`/`appendEventTopic` are preserved). `--add` is idempotent on `topicFilter`, with `--qos` (0-2) and `--retain` (none|property|all). Backed by a new `setEventPublicationConfig` call in `vapix/mqtt.rs`. Enables Axis Object Analytics events to actually be published to MQTT (Axis only starts publishing once a filter is set).
- Offline unit-test coverage for error sanitization, value encoding, parameter parsing, and TLS-default behavior.

## v0.19.0

### Security
- **TLS verification secure by default**: `verify_ssl` now defaults to `true` for HTTPS connections (previously `false`). A warning is printed when certificate verification is disabled. Use `-k`/`--insecure` per command, or `verify_ssl: false` in `cameras.yaml`, to opt out. **Behavior change**: HTTPS cameras with self-signed certificates now require `-k`/`--insecure` or `verify_ssl: false`.
- **Sanitized firmware/clip error responses**: Error bodies from multipart upload endpoints (`fw upgrade`, `clip upload`) are now passed through `sanitize_error_body()` like all other endpoints, stripping raw HTML/server internals from error output.
- **Missing env-var warning**: Unset `${VAR}` references in `cameras.yaml` now log a warning instead of silently substituting an empty string (which could create empty passwords).

### Changed
- **Replaced unmaintained `atty` dependency** with `std::io::IsTerminal` (Rust standard library), resolving RUSTSEC-2021-0145.

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
