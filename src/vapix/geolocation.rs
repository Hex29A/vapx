//! Geolocation (`/axis-cgi/geolocation`).

use serde_json::{Map, Value};

use crate::vapix::client::VapixClient;

/// Current location settings: one member per XML element, with booleans and
/// numbers converted.
pub fn get_location(client: &VapixClient) -> anyhow::Result<Value> {
    parse_location(&client.get_text("/axis-cgi/geolocation/get.cgi", &[])?)
}

/// Every leaf element of the answer becomes a member, whatever the nesting.
/// Older firmware answers `<GeoLocation><Latitude>..</Latitude>..</GeoLocation>`;
/// AXIS OS 10.12 nests the same kind of values as
/// `<PositionResponse><Success><GetSuccess><Location><Lat>..`, which used to
/// come out as `{"Success": "\n"}`. A `Failure` element is an error.
fn parse_location(xml: &str) -> anyhow::Result<Value> {
    let doc = roxmltree::Document::parse(xml)?;
    if let Some(failure) = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "Failure") {
        let detail: Vec<&str> = failure.descendants().filter_map(|n| n.text()).map(str::trim).filter(|t| !t.is_empty()).collect();
        anyhow::bail!("Geolocation error: {}", detail.join(" "));
    }
    let mut out = Map::new();
    for leaf in doc.root_element().descendants().filter(|n| n.is_element() && !n.children().any(|c| c.is_element())) {
        let text = leaf.text().unwrap_or("").trim();
        let value = match text.to_ascii_lowercase().as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => match text.parse::<f64>() {
                Ok(n) => serde_json::Number::from_f64(n).map(Value::Number).unwrap_or_else(|| Value::String(text.into())),
                Err(_) => Value::String(text.into()),
            },
        };
        out.insert(leaf.tag_name().name().to_string(), value);
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

    /// The answer of AXIS OS 10.12.363 (a camera with no position set).
    const ANSWER_10_12: &str = "<PositionResponse SchemaVersion=\"1.0\">\n<Success>\n<GetSuccess>\n<Location>\n<Lat>00.000000000</Lat>\n<Lng>000.000000000</Lng>\n<Heading>0.000000</Heading>\n</Location>\n<Text></Text>\n<ValidPosition>false</ValidPosition>\n<ValidHeading>false</ValidHeading>\n</GetSuccess>\n</Success>\n</PositionResponse>\n";

    #[test]
    fn the_nested_answer_of_10_12_is_read() {
        let v = parse_location(ANSWER_10_12).unwrap();
        assert_eq!(v["Lat"], 0.0);
        assert_eq!(v["Lng"], 0.0);
        assert_eq!(v["Heading"], 0.0);
        assert_eq!(v["Text"], "");
        assert_eq!(v["ValidPosition"], false);
        assert_eq!(v["ValidHeading"], false);
        assert!(v.get("Success").is_none());
    }

    #[test]
    fn a_failure_answer_is_an_error() {
        let xml = "<PositionResponse><Failure><Error><Code>1</Code><Description>Not allowed</Description></Error></Failure></PositionResponse>";
        let e = parse_location(xml).unwrap_err().to_string();
        assert!(e.contains("Not allowed"), "{e}");
    }
}
