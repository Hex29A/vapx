//! Stream URLs (RTSP, MJPEG, JPEG snapshot). Pure string building: nothing is
//! requested from the camera.

/// Options shared by the URL builders.
#[derive(Default)]
pub struct StreamOpts<'a> {
    pub channel: u8,
    pub resolution: Option<&'a str>,
    pub codec: Option<&'a str>,
    pub fps: Option<u8>,
    pub stream_profile: u8,
}

/// `user:pass@` for the URL, when credentials are to be embedded.
fn userinfo(auth: Option<(&str, &str)>) -> String {
    auth.map(|(u, p)| format!("{u}:{p}@")).unwrap_or_default()
}

pub fn rtsp(host: &str, auth: Option<(&str, &str)>, o: &StreamOpts) -> String {
    let mut path = format!("/axis-media/media.amp?videocodec={}&camera={}", o.codec.unwrap_or("h264"), o.channel);
    if let Some(res) = o.resolution {
        path.push_str(&format!("&resolution={res}"));
    }
    if let Some(fps) = o.fps {
        path.push_str(&format!("&fps={fps}"));
    }
    if o.stream_profile > 0 {
        path.push_str(&format!("&streamprofile={}", o.stream_profile));
    }
    format!("rtsp://{}{}:554{}", userinfo(auth), host, path)
}

pub fn mjpeg(https: bool, host: &str, port: u16, auth: Option<(&str, &str)>, o: &StreamOpts) -> String {
    let mut params = format!("camera={}", o.channel);
    if let Some(res) = o.resolution {
        params.push_str(&format!("&resolution={res}"));
    }
    if let Some(fps) = o.fps {
        params.push_str(&format!("&fps={fps}"));
    }
    let scheme = if https { "https" } else { "http" };
    format!("{scheme}://{}{host}:{port}/axis-cgi/mjpg/video.cgi?{params}", userinfo(auth))
}

pub fn snapshot(https: bool, host: &str, port: u16, auth: Option<(&str, &str)>, o: &StreamOpts) -> String {
    let mut params = format!("camera={}", o.channel);
    if let Some(res) = o.resolution {
        params.push_str(&format!("&resolution={res}"));
    }
    let scheme = if https { "https" } else { "http" };
    format!("{scheme}://{}{host}:{port}/axis-cgi/jpg/image.cgi?{params}", userinfo(auth))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> StreamOpts<'static> {
        StreamOpts { channel: 1, resolution: Some("1280x720"), codec: None, fps: Some(15), stream_profile: 2 }
    }

    #[test]
    fn rtsp_url_without_credentials() {
        assert_eq!(
            rtsp("192.0.2.10", None, &opts()),
            "rtsp://192.0.2.10:554/axis-media/media.amp?videocodec=h264&camera=1&resolution=1280x720&fps=15&streamprofile=2"
        );
    }

    #[test]
    fn credentials_appear_only_when_asked_for() {
        let with = mjpeg(true, "192.0.2.10", 443, Some(("root", "pass")), &opts());
        assert_eq!(with, "https://root:pass@192.0.2.10:443/axis-cgi/mjpg/video.cgi?camera=1&resolution=1280x720&fps=15");
        let without = snapshot(false, "192.0.2.10", 80, None, &opts());
        assert_eq!(without, "http://192.0.2.10:80/axis-cgi/jpg/image.cgi?camera=1&resolution=1280x720");
    }
}
