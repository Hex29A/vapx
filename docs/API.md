# VAPIX API reference

This maps every `vapx` command to the VAPIX CGI endpoint(s) it calls, so you
can tell what a command actually does on the wire, and reuse the mapping if
you're calling VAPIX directly instead of through `vapx`. Endpoint behavior
(parameters, response format) is Axis's, documented at
[developer.axis.com/vapix](https://developer.axis.com/vapix/) — this page
only lists which endpoint each command uses and how vapx uses it.

Purely local commands (`config`, `stream`, `completions`, `mangen`) make no
network call and are omitted below.

| Command | Endpoint(s) | Notes |
|---|---|---|
| `info` | `basicdeviceinfo.cgi` | POST JSON-RPC, `getAllProperties` |
| `snap` | `jpg/image.cgi` | GET, raw JPEG bytes |
| `fw` | `firmwaremanagement.cgi` | POST JSON-RPC; one method per subcommand (status/upgrade/commit/rollback/reboot/factoryDefault/check) |
| `acap` | `applications/list.cgi`, `applications/control.cgi` | list is GET, control (start/stop/restart/remove) is GET with `action` + `package` params |
| `ptz` | `com/ptz.cgi` | GET, legacy VAPIX PTZ syntax (move/goto/preset/query/info all go through this one endpoint) |
| `param` | `param.cgi` | GET (list/get) and POST (set) |
| `net` | `param.cgi` | scoped to the `root.Network` group |
| `time` | `param.cgi` | scoped to the `root.Time` group |
| `hw` | `param.cgi`, `io/portmanagement.cgi`, `io/port.cgi` | `show` reads `root.IOPort` params and port config; `trigger` calls `io/port.cgi?action=<idx>:<state>` |
| `user` | `pwdgrp.cgi` | GET (list) and POST (add/update/remove) |
| `pass` | `pwdgrp.cgi` | POST, `update` action on the caller's own or another account |
| `events` | `wssession.cgi` + WebSocket | `wssession.cgi` mints a session token, then vapx opens a WS connection with it |
| `watch` | `wssession.cgi` + WebSocket | same as `events`, fanned out over multiple cameras in parallel |
| `discover` | `apidiscovery.cgi` | POST JSON-RPC, lists which VAPIX APIs (and versions) the device supports |
| `diff` | `param.cgi` | pulls the full parameter tree from two devices (or a group) and diffs client-side — no diff-specific endpoint |
| `backup` | `param.cgi` | `save` reads the full tree, `restore` writes it back parameter by parameter |
| `template` | `param.cgi` | same primitive as `backup`, applied against a saved desired-state file instead of another camera |
| `overlay` | `dynamicoverlay/dynamicoverlay.cgi` | POST JSON-RPC |
| `cert` | `certificate/certificate.cgi`, `certmanagement.cgi` | listing/removal via `certificate.cgi`, CSR/self-sign via `certmanagement.cgi` |
| `rule` | `action/action.cgi` | POST JSON-RPC (ActionConfiguration service) |
| `storage` | `disks/list.cgi`, `disks/properties.cgi`, `record/list.cgi`, `param.cgi` | `properties.cgi` (per-disk health) falls back to `list.cgi` data when it 404s (older firmware) |
| `health` | `basicdeviceinfo.cgi` (+ latency probe) | fleet-wide fan-out of `info`, run in parallel per camera/group |
| `temp` | `temperaturecontrol.cgi` | GET, key=value response, not JSON |
| `daynight` | `param.cgi` | `root.ImageSource.*.DayNight` group |
| `imaging` | `param.cgi` | `root.ImageSource.*` sensor params (brightness/contrast/exposure/WDR) |
| `light` | `param.cgi` | `root.Image.I0.IrCutFilter` / IR illuminator params |
| `vmd` | `param.cgi` | `root.Motion` / VMD params |
| `audio` | `param.cgi` | `root.Audio` source params |
| `clip` | `mediaclip.cgi`, `param.cgi` | list/play/upload/delete via `mediaclip.cgi`; naming via `param.cgi` |
| `mqtt` | `mqtt/client.cgi`, `mqtt/event.cgi` | client lifecycle (status/configure/enable/disable) vs. event publication filters, two separate JSON-RPC services |
| `streamstatus` | `streamstatus.cgi` (WebSocket) with `param.cgi` fallback | on firmware where `streamstatus.cgi` needs WS transport and that also fails, vapx shows stream *configuration* from `param.cgi` instead and says so |
| `selftest` | `deviceselftest.cgi` | POST JSON-RPC, requires the device to be in preview mode |
| `signedvideo` | `signedvideo.cgi`, `param.cgi` | status/enable/disable via `signedvideo.cgi`; falls back to `param.cgi` if that 404s |
| `zipstream` | `zipstream/listprofiles.cgi`, `zipstream/setprofile.cgi` | POST JSON-RPC |
| `viewarea` | `viewarea/info.cgi` | POST JSON-RPC (list/get/set geometry) |
| `audit` | composite | reads params, users, and other state already covered above and scores it — no endpoint of its own |
| `log` | `systemlog.cgi`, `accesslog.cgi`, `serverreport.cgi` | GET, plain text |
| `systemready` | `systemready.cgi` | GET, the one endpoint that answers without credentials — used both standalone and internally by `enroll` |
| `enroll` | `systemready.cgi`, `pwdgrp.cgi`, `basicdeviceinfo.cgi` | checks readiness, creates the initial account, then re-reads device info now that credentials work (needed to derive the config name) |

## Auth

Every endpoint above goes through the same negotiation vapx always uses:
Digest over HTTP, Basic over HTTPS (see `README.md#authentication`). The only
endpoint that skips auth entirely is `systemready.cgi` — that's what makes it
usable on a factory-default camera before any account exists.

## Where this lives in the source

Each row's endpoint calls live in `src/vapix/<module>.rs`; the CLI parsing and
flag handling for the matching command lives in `src/cmd/<command>.rs`. See
`README.md#architecture` for the full file listing.
