# AXIS OS 10.12 responses

Recorded from an AXIS M3115-LVE on AXIS OS 10.12.363 through a logging proxy,
while running vapx 0.35.2. Used by `src/vapix/fixtures_10_12.rs`.

Sanitized: the serial number, SoC serial, MAC and IP addresses and the account
names were replaced with neutral values (`TESTSERIAL01`, `192.0.2.x`, MAC addresses as `<mac>`,
`operator1`). Everything else is as the camera sent it.

| File | Request |
|------|---------|
| `basicdeviceinfo.json` | `basicdeviceinfo.cgi` getAllProperties |
| `systemready.json` | `systemready.cgi` (no uptime, bootid or passphrase policy) |
| `firmware_status.json` | `firmwaremanagement.cgi` status |
| `apidiscovery.json` | `apidiscovery.cgi` getApiList (mqtt-client 1.5, no streamstatus) |
| `pwdgrp_get.txt` | `pwdgrp.cgi?action=get` (all system groups, web-access groups in the role lines) |
| `mqtt_client_status_1_6.json` | `mqtt/client.cgi` getClientStatus sent with apiVersion 1.6: error 2100 |
| `mqtt_event_config.json` | `mqtt/event.cgi` getEventPublicationConfig |
| `not_found.html` | the 404 page (cert REST API, certificate.cgi, action.cgi, properties.cgi, ...) |
| `disks_list.xml` | `disks/list.cgi?diskid=all` |
| `temperaturecontrol.txt` | `temperaturecontrol.cgi?method=getSensorList` |
| `geolocation.xml` | `geolocation/get.cgi` (no position set) |
| `action_rules_soap.xml` | `/vapix/services` GetActionRules (no rules) |
| `viewarea_list.json` | `viewarea/info.cgi` list |
| `applications_list.xml` | `applications/list.cgi` |
| `param_ssh.txt` | `param.cgi?action=list&group=root.Network.SSH` |
