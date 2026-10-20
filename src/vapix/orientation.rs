//! Orientation (tilt) readings from `/axis-cgi/orientation`.

use serde_json::{json, Value};

use crate::vapix::client::VapixClient;

/// `{available, longitudinal, lateral}`. `available` is false when the camera
/// has no orientation sensor.
pub fn get_orientation(client: &VapixClient) -> Value {
    let longitudinal = read_value(client, "/axis-cgi/orientation/getlongitudinalvalue.cgi");
    let lateral = read_value(client, "/axis-cgi/orientation/getlateralvalue.cgi");
    json!({
        "available": longitudinal.is_some(),
        "longitudinal": longitudinal,
        "lateral": lateral,
    })
}

fn read_value(client: &VapixClient, path: &str) -> Option<f64> {
    let text = client.get_text(path, &[("schemaversion", "1")]).ok()?;
    parse_value(&text)
}

fn parse_value(xml: &str) -> Option<f64> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    doc.descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "Value")
        .and_then(|n| n.text())
        .and_then(|t| t.trim().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::parse_value;

    #[test]
    fn reads_the_first_value_element() {
        assert_eq!(parse_value("<a><Value>1.5</Value></a>"), Some(1.5));
        assert_eq!(parse_value("<a><Other/></a>"), None);
        assert_eq!(parse_value("not xml"), None);
    }
}
