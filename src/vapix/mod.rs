pub mod analyticsmetadata;
pub mod applications;
pub mod audio;
pub mod audio_clip;
pub mod auth;
pub mod capturemode;
pub mod certs;
pub mod clearview;
pub mod client;
pub mod daynight;
pub mod device;
pub mod discover;
pub mod disk;
pub mod events;
pub mod firmware;
pub mod geolocation;
pub mod guardtour;
pub mod image;
pub mod io;
pub mod lightcontrol;
pub mod mqtt;
pub mod network;
pub mod orientation;
pub mod overlay;
pub mod params;
pub mod privacymask;
pub mod ptz;
pub mod rules;
pub mod selftest;
pub mod signedvideo;
pub mod siren;
pub mod ssh;
pub mod storage;
pub mod streamprofile;
pub mod streamstatus;
pub mod streamurl;
pub mod system;
pub mod systemready;
pub mod temperature;
pub mod time;
pub mod users;
pub mod viewarea;
pub mod vmd;
pub mod ws;
pub mod zipstream;

/// The `data` member of a VAPIX JSON response, or the whole response when
/// there is none.
pub fn data_of(resp: serde_json::Value) -> serde_json::Value {
    match resp {
        serde_json::Value::Object(mut m) if m.contains_key("data") => m.remove("data").unwrap(),
        other => other,
    }
}
