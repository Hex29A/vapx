//! Day/night (IR-cut filter) configuration.

use serde_json::{json, Map, Value};

use crate::vapix::client::VapixClient;

const GROUP: &str = "ImageSource.I0.DayNight";

/// Configuration for `channel`: the `daynight.cgi` API, or `param.cgi` on
/// cameras that do not have it.
pub fn get_configuration(client: &VapixClient, channel: i64) -> anyhow::Result<Value> {
    let modern = client.post_json(
        "/axis-cgi/daynight.cgi",
        &json!({"apiVersion": "1.2", "method": "getConfiguration", "params": {"channel": channel}}),
    );
    match modern {
        Ok(resp) => Ok(super::data_of(resp)),
        Err(_) => {
            let text = client.get_text("/axis-cgi/param.cgi", &[("action", "list"), ("group", GROUP)])?;
            Ok(parse_params(&text))
        }
    }
}

/// `root.ImageSource.I0.DayNight.IrCutFilter=auto` → `{"IrCutFilter": "auto"}`.
fn parse_params(text: &str) -> Value {
    let mut out = Map::new();
    for line in text.lines() {
        if let Some((key, value)) = line.trim().split_once('=') {
            let short = key.rsplit('.').next().unwrap_or(key);
            out.insert(short.to_string(), json!(value));
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::parse_params;

    #[test]
    fn keeps_the_last_key_segment() {
        let v = parse_params("root.ImageSource.I0.DayNight.IrCutFilter=auto\nbad line\n");
        assert_eq!(v["IrCutFilter"], "auto");
        assert_eq!(v.as_object().unwrap().len(), 1);
    }
}
