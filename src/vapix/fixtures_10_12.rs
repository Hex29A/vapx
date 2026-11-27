//! A camera on AXIS OS 10.12, answering with responses recorded from a real
//! device (see `testdata/axisos-10.12/README.md`). The tests drive the public
//! functions against it, so the code paths for older firmware are covered in CI
//! without a camera: MQTT API 1.5, no certificate API, the user list with
//! system groups, the SOAP action rules, disks without properties.cgi and SSH
//! without the account API.

use crate::vapix::testserver::{Reply, TestServer};
use crate::vapix::{
    applications, certs, device, discover, firmware, geolocation, mqtt, rules, ssh, storage, systemready,
    temperature, users, viewarea,
};

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!("testdata/axisos-10.12/", $name))
    };
}

const NOT_FOUND: &str = fixture!("not_found.html");

/// Route one request the way the recorded camera answered it. Anything the
/// recording does not have is a 404, as the camera answers an unknown CGI.
fn answer(req: &str) -> Reply {
    let line = req.lines().next().unwrap_or("");
    let get = line.starts_with("GET ");
    let has = |s: &str| line.contains(s);
    if has("/axis-cgi/basicdeviceinfo.cgi") {
        Reply::Json(200, fixture!("basicdeviceinfo.json"))
    } else if has("/axis-cgi/systemready.cgi") {
        Reply::Json(200, fixture!("systemready.json"))
    } else if has("/axis-cgi/firmwaremanagement.cgi") {
        Reply::Json(200, fixture!("firmware_status.json"))
    } else if has("/axis-cgi/apidiscovery.cgi") {
        Reply::Json(200, fixture!("apidiscovery.json"))
    } else if has("/axis-cgi/pwdgrp.cgi") {
        Reply::Text(200, fixture!("pwdgrp_get.txt"))
    } else if has("/axis-cgi/mqtt/client.cgi") {
        // Recorded: the answer to 1.6. The camera accepts the 1.5 it lists.
        if req.contains(r#""apiVersion":"1.5""#) {
            Reply::Json(200, r#"{"apiVersion":"1.5","method":"getClientStatus","data":{"status":{"state":"inactive","connectionStatus":"disconnected"}}}"#)
        } else {
            Reply::Json(200, fixture!("mqtt_client_status_1_6.json"))
        }
    } else if has("/axis-cgi/mqtt/event.cgi") {
        Reply::Json(200, fixture!("mqtt_event_config.json"))
    } else if has("/axis-cgi/disks/list.cgi") && get {
        Reply::Text(200, fixture!("disks_list.xml"))
    } else if has("/axis-cgi/disks/list.cgi") {
        Reply::Text(400, "")
    } else if has("/axis-cgi/temperaturecontrol.cgi") {
        Reply::Text(200, fixture!("temperaturecontrol.txt"))
    } else if has("/axis-cgi/geolocation/get.cgi") {
        Reply::Text(200, fixture!("geolocation.xml"))
    } else if has("/vapix/services") {
        Reply::Text(200, fixture!("action_rules_soap.xml"))
    } else if has("/axis-cgi/viewarea/info.cgi") {
        Reply::Json(200, fixture!("viewarea_list.json"))
    } else if has("/axis-cgi/applications/list.cgi") {
        Reply::Text(200, fixture!("applications_list.xml"))
    } else if has("/axis-cgi/param.cgi") && has("root.Network.SSH") {
        Reply::Text(200, fixture!("param_ssh.txt"))
    } else if has("/axis-cgi/param.cgi") && has("Properties.Firmware.Version") {
        Reply::Text(200, "root.Properties.Firmware.Version=10.12.363\n")
    } else {
        Reply::Text(404, NOT_FOUND)
    }
}

fn camera() -> TestServer {
    TestServer::start(answer)
}

#[test]
fn device_info_reports_the_firmware() {
    let resp = device::get_all_properties(&camera().client()).unwrap();
    assert_eq!(resp.pointer("/data/propertyList/Version").and_then(|v| v.as_str()), Some("10.12.363"));
    assert_eq!(resp.pointer("/data/propertyList/ProdNbr").and_then(|v| v.as_str()), Some("M3115-LVE"));
}

#[test]
fn systemready_without_the_newer_fields() {
    let state = systemready::query(&camera().client(), 10).unwrap();
    assert!(state.systemready && !state.needsetup);
    assert_eq!(state.uptime, None);
    assert_eq!(state.bootid, None);
}

#[test]
fn firmware_status_without_commit_fields() {
    let c = camera().client();
    assert_eq!(firmware::active_version(&c).unwrap(), "10.12.363");
    assert_eq!(firmware::time_to_rollback(&c), None);
}

#[test]
fn discovery_lists_mqtt_1_5_and_no_streamstatus() {
    let resp = discover::get_api_list(&camera().client()).unwrap();
    assert_eq!(discover::version_in_list(&resp, "mqtt-client").as_deref(), Some("1.5"));
    assert_eq!(discover::version_in_list(&resp, "streamstatus"), None);
}

/// 0.35.2 always sent 1.6 and got the recorded error 2100.
#[test]
fn mqtt_uses_the_listed_1_5() {
    let srv = camera();
    let c = srv.client();
    mqtt::get_client_status(&c).unwrap();
    mqtt::get_event_config(&c).unwrap();
    let sent = srv.requests().into_iter().find(|r| r.contains("client.cgi")).unwrap();
    assert!(sent.contains(r#""apiVersion":"1.5""#), "{sent}");
}

#[test]
fn the_recorded_answer_to_mqtt_1_6_is_an_error() {
    let srv = TestServer::start(|_| Reply::Json(200, fixture!("mqtt_client_status_1_6.json")));
    let err = srv.client().post_json("/axis-cgi/mqtt/client.cgi", &serde_json::json!({"apiVersion": "1.6"})).unwrap_err();
    assert!(err.to_string().contains("2100"), "{err}");
}

#[test]
fn cert_list_says_it_needs_axis_os_12() {
    let err = certs::list(&camera().client()).unwrap_err().to_string();
    assert!(err.contains("12"), "{err}");
}

/// The role lines are mixed with web-access groups (`wwwa`, `wwwaop`, ...)
/// and system users (`sdk`, `vdo`, `debugar`), and every system group is
/// listed. Until 0.36.1 all of them came out as accounts.
#[test]
fn user_list_has_only_the_accounts() {
    let v = users::get_users(&camera().client()).unwrap();
    let names: Vec<&str> = v["users"].as_array().unwrap().iter().filter_map(|u| u["username"].as_str()).collect();
    assert_eq!(names, ["operator1", "root"]);
    let admins: Vec<&str> = v["groups"]["admin"].as_array().unwrap().iter().filter_map(|u| u.as_str()).collect();
    assert_eq!(admins, ["operator1", "root"]);
    let operators: Vec<&str> = v["groups"]["operator"].as_array().unwrap().iter().filter_map(|u| u.as_str()).collect();
    assert_eq!(operators, ["root", "operator1"]);
    assert!(v["groups"].get("tty").is_none(), "system groups are left out: {}", v["groups"]);
}

/// `debugar` in the admin line used to count as another admin.
#[test]
fn the_last_admin_check_ignores_system_users() {
    let text = fixture!("pwdgrp_get.txt");
    let r = users::plan_removal(text, "operator1", "root");
    assert!(r.exists && !r.last_admin, "root is still an admin");
    let r = users::plan_removal(text, "root", "operator1");
    assert!(!r.last_admin, "operator1 is still an admin");
    let only_root = text.replace("admin=\"wwwaop,wwwaov,operator1,", "admin=\"wwwaop,wwwaov,");
    let r = users::plan_removal(&only_root, "root", "operator1");
    assert!(r.last_admin, "debugar and sdk are no admins");
}

#[test]
fn action_rules_come_from_soap() {
    let resp = rules::list_rules(&camera().client()).unwrap();
    assert_eq!(resp.pointer("/data/source").and_then(|v| v.as_str()), Some("soap"));
    assert_eq!(resp.pointer("/data/rules").and_then(|v| v.as_array()).map(Vec::len), Some(0));
}

#[test]
fn disk_health_comes_from_the_list_without_properties_cgi() {
    let health = storage::get_disk_health(&camera().client()).unwrap();
    let text = health.to_string();
    assert!(text.contains("SD_DISK") && text.contains("NetworkShare"), "{text}");
}

#[test]
fn temperature_sensors_and_heater_are_read() {
    let v = temperature::parse_sensors(&temperature::get_sensors(&camera().client()).unwrap());
    let text = v.to_string();
    assert!(text.contains("CPU") && text.contains("40.2") && text.contains("Heater"), "{text}");
}

#[test]
fn geolocation_without_a_position() {
    let v = geolocation::get_location(&camera().client()).unwrap();
    assert_eq!(v["ValidPosition"], serde_json::json!(false), "{v}");
}

#[test]
fn view_areas_and_applications_are_listed() {
    let c = camera().client();
    let areas = viewarea::list(&c).unwrap();
    assert!(areas.pointer("/data/viewAreas/0/id").is_some(), "{areas}");
    let apps = applications::list_applications(&c).unwrap();
    assert_eq!(apps.len(), 1);
    assert_eq!(apps[0].name, "vmd");
}

#[test]
fn ssh_is_root_only_without_the_account_api() {
    let c = camera().client();
    assert!(!ssh::is_enabled(&c).unwrap());
    assert_eq!(ssh::api_versions(&c), None);
    let fw = ssh::firmware_version(&c).unwrap();
    assert_eq!(ssh::login_mode(&fw), Some("root"));
}
