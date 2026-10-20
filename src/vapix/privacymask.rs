//! Privacy masks (`privacymask.cgi`).

use anyhow::Context;
use serde_json::Value;

use crate::vapix::client::VapixClient;

/// The configured masks (`listpx`), empty when there are none.
pub fn list_masks(client: &VapixClient) -> anyhow::Result<Value> {
    let text = client.get_text("/axis-cgi/privacymask.cgi", &[("query", "listpxjson")])?;
    parse_masks(&text)
}

fn parse_masks(text: &str) -> anyhow::Result<Value> {
    let mut v: Value = serde_json::from_str(text).context("Privacy mask list is not JSON")?;
    Ok(v.get_mut("listpx").map(Value::take).unwrap_or_else(|| Value::Array(vec![])))
}

const PATH: &str = "/axis-cgi/privacymask.cgi";

/// Add a mask called `name` covering `polygon` (`"x,y:x,y:..."` in sensor pixels).
pub fn add_mask(client: &VapixClient, name: &str, polygon: &str) -> anyhow::Result<()> {
    mask_command(client, &[("action", "add"), ("name", name), ("imagesource", "0"), ("pxpolygon", polygon)])
}

pub fn remove_mask(client: &VapixClient, name: &str) -> anyhow::Result<()> {
    mask_command(client, &[("action", "remove"), ("name", name)])
}

fn mask_command(client: &VapixClient, params: &[(&str, &str)]) -> anyhow::Result<()> {
    let reply = client.get_text(PATH, params)?;
    if reply.trim().starts_with("Error") {
        anyhow::bail!("Privacy mask error: {}", reply.trim());
    }
    Ok(())
}

/// The sensor's resolution from `param.cgi`, or 1920x1080 when it cannot be read.
pub fn sensor_resolution(client: &VapixClient) -> (i64, i64) {
    let read = || -> Option<(i64, i64)> {
        let text = client
            .get_text("/axis-cgi/param.cgi", &[("action", "list"), ("group", "Image.I0.Appearance.Resolution")])
            .ok()?;
        let res = text.trim().rsplit('=').next()?.trim().to_string();
        let (w, h) = res.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    };
    read().unwrap_or((1920, 1080))
}

/// Convert a polygon given in the coordinates of the displayed image (after the
/// camera's rotation and mirroring) to sensor coordinates.
pub fn display_to_sensor(
    polygon: &str,
    rotation: u16,
    mirror: bool,
    sensor: (i64, i64),
    display: (i64, i64),
) -> anyhow::Result<String> {
    let mut points: Vec<(i64, i64)> = Vec::new();
    for pt in polygon.split(':') {
        let (x, y) = pt.split_once(',').ok_or_else(|| anyhow::anyhow!("Invalid polygon point '{}'", pt))?;
        points.push((x.trim().parse()?, y.trim().parse()?));
    }
    let (sw, sh) = sensor;
    let (dw, dh) = display;
    if dw <= 0 || dh <= 0 {
        anyhow::bail!("display size must be positive");
    }
    let (sx, sy) = if rotation == 0 || rotation == 180 {
        (sw as f64 / dw as f64, sh as f64 / dh as f64)
    } else {
        (sh as f64 / dw as f64, sw as f64 / dh as f64)
    };
    let mut points: Vec<(i64, i64)> = points.iter().map(|&(x, y)| ((x as f64 * sx) as i64, (y as f64 * sy) as i64)).collect();
    match rotation {
        180 => points = points.iter().map(|&(x, y)| (sw - x, sh - y)).collect(),
        90 => points = points.iter().map(|&(x, y)| (y, sw - x)).collect(),
        270 => points = points.iter().map(|&(x, y)| (sh - y, x)).collect(),
        _ => {}
    }
    if mirror {
        points = points.iter().map(|&(x, y)| (sw - x, y)).collect();
    }
    Ok(points.iter().map(|(x, y)| format!("{x},{y}")).collect::<Vec<_>>().join(":"))
}

#[cfg(test)]
mod tests {
    use super::parse_masks;

    #[test]
    fn extracts_listpx() {
        let v = parse_masks(r#"{"listpx":[{"name":"m1"}]}"#).unwrap();
        assert_eq!(v[0]["name"], "m1");
        assert_eq!(parse_masks("{}").unwrap(), serde_json::json!([]));
        assert!(parse_masks("nope").is_err());
    }

    #[test]
    fn unrotated_polygons_are_only_scaled() {
        let p = super::display_to_sensor("0,0:960,540", 0, false, (1920, 1080), (960, 540)).unwrap();
        assert_eq!(p, "0,0:1920,1080");
    }

    #[test]
    fn rotation_and_mirroring_move_the_points() {
        // 180 degrees: a point at the top left lands at the bottom right.
        let p = super::display_to_sensor("100,100", 180, false, (1920, 1080), (1920, 1080)).unwrap();
        assert_eq!(p, "1820,980");
        // Mirroring flips x only.
        let p = super::display_to_sensor("100,100", 0, true, (1920, 1080), (1920, 1080)).unwrap();
        assert_eq!(p, "1820,100");
        // 90 degrees swaps the axes.
        let p = super::display_to_sensor("100,200", 90, false, (1920, 1080), (1080, 1920)).unwrap();
        assert_eq!(p, "200,1820");
    }

    #[test]
    fn bad_polygons_are_rejected() {
        assert!(super::display_to_sensor("1;2", 0, false, (10, 10), (10, 10)).is_err());
        assert!(super::display_to_sensor("a,b", 0, false, (10, 10), (10, 10)).is_err());
        assert!(super::display_to_sensor("1,2", 0, false, (10, 10), (0, 10)).is_err());
    }
}
