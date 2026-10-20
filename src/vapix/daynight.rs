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

/// Apply day/night settings to `channel`: the `daynight.cgi` API, or one
/// `param.cgi` update per setting on cameras without it.
pub fn set_configuration(client: &VapixClient, channel: i64, settings: &Map<String, Value>) -> anyhow::Result<()> {
    let mut params = Map::new();
    params.insert("channel".into(), json!(channel));
    params.extend(settings.clone());
    let modern = client.post_json(
        "/axis-cgi/daynight.cgi",
        &json!({"apiVersion": "1.2", "method": "setConfiguration", "params": params}),
    );
    if modern.is_ok() {
        return Ok(());
    }
    for (key, value) in settings {
        let name = format!("{GROUP}.{key}");
        let text = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        client.get_text("/axis-cgi/param.cgi", &[("action", "update"), (&name, &text)])?;
    }
    Ok(())
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
