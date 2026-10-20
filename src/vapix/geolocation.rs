//! Geolocation (`/axis-cgi/geolocation`).

use serde_json::{Map, Value};

use crate::vapix::client::VapixClient;

/// Current location settings: one member per XML element, with booleans and
/// numbers converted.
pub fn get_location(client: &VapixClient) -> anyhow::Result<Value> {
    parse_location(&client.get_text("/axis-cgi/geolocation/get.cgi", &[])?)
}

fn parse_location(xml: &str) -> anyhow::Result<Value> {
    let doc = roxmltree::Document::parse(xml)?;
    let mut out = Map::new();
    for child in doc.root_element().children().filter(|n| n.is_element()) {
        let text = child.text().unwrap_or("");
        let value = match text.to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => match text.parse::<f64>() {
                Ok(n) => serde_json::Number::from_f64(n).map(Value::Number).unwrap_or_else(|| Value::String(text.into())),
                Err(_) => Value::String(text.into()),
            },
        };
        out.insert(child.tag_name().name().to_string(), value);
    }
    Ok(Value::Object(out))
}

/// Set position (WGS-84 decimal degrees), heading and free text. Only the
/// given values change.
pub fn set_location(
    client: &VapixClient,
    lat: Option<f64>,
    lng: Option<f64>,
    heading: Option<f64>,
    text: Option<&str>,
) -> anyhow::Result<()> {
    let numbers: Vec<(&str, String)> = [("lat", lat), ("lng", lng), ("heading", heading)]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v.to_string())))
        .collect();
    let mut params: Vec<(&str, &str)> = numbers.iter().map(|(k, v)| (*k, v.as_str())).collect();
    if let Some(t) = text {
        params.push(("text", t));
    }
    let reply = client.get_text("/axis-cgi/geolocation/set.cgi", &params)?;
    if reply.to_lowercase().contains("error") {
        anyhow::bail!("Geolocation error: {}", reply.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numbers_booleans_and_text() {
        let xml = r#"<GeoLocation xmlns="http://www.axis.com/vapix/http_cgi/geolocation1">
            <Latitude>59.3293</Latitude><Valid>true</Valid><Text>Front door</Text></GeoLocation>"#;
        let v = parse_location(xml).unwrap();
        assert_eq!(v["Latitude"], 59.3293);
        assert_eq!(v["Valid"], true);
        assert_eq!(v["Text"], "Front door");
    }
}
