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
}
