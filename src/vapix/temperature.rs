use anyhow::bail;

use crate::vapix::client::VapixClient;

/// Fetch temperature sensor data from temperaturecontrol.cgi.
/// Returns the raw key=value text response.
pub fn get_sensors(client: &VapixClient) -> anyhow::Result<String> {
    let resp = client.get_text(
        "/axis-cgi/temperaturecontrol.cgi",
        &[("method", "getSensorList")],
    );
    match resp {
        Ok(text) => {
            if text.starts_with("# Error:") || text.contains("Error:") {
                bail!("temperaturecontrol.cgi: {}", text.trim());
            }
            Ok(text)
        }
        Err(e) => {
            let msg = format!("{}", e);
            if msg.contains("404") {
                bail!(
                    "Temperature sensor API not available on this camera. \
                     Requires AXIS OS 10.x or newer with temperature sensors."
                )
            } else {
                Err(e)
            }
        }
    }
}

/// Parse the temperaturecontrol.cgi key=value response into structured JSON.
/// Format: Sensor.S0.Name=CPU, Sensor.S0.Celsius=30.2, Sensor.S0.Fahrenheit=86.4
/// Also supports: root.TemperatureControl.TemperatureSensor.0.Name=CPU (older firmware)
pub fn parse_sensors(text: &str) -> serde_json::Value {
    let mut sensors: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> =
        std::collections::BTreeMap::new();
    let mut heaters: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> =
        std::collections::BTreeMap::new();
    let mut extras = serde_json::Map::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        // Modern format: Sensor.S0.Name=CPU / Sensor.S0.Celsius=30.2
        if let Some(rest) = key.strip_prefix("Sensor.") {
            if let Some((id, field)) = rest.split_once('.') {
                let entry = sensors.entry(id.to_string()).or_default();
                match field {
                    "Celsius" | "Fahrenheit" => {
                        if let Ok(v) = value.parse::<f64>() {
                            entry.insert(
                                field.to_lowercase(),
                                serde_json::json!((v * 10.0).round() / 10.0),
                            );
                        }
                    }
                    _ => {
                        entry.insert(
                            field.to_lowercase(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }
            }
        }
        // Heater entries: Heater.H0.Status=Stopped
        else if let Some(rest) = key.strip_prefix("Heater.") {
            if let Some((id, field)) = rest.split_once('.') {
                let entry = heaters.entry(id.to_string()).or_default();
                entry.insert("id".to_string(), serde_json::Value::String(id.to_string()));
                entry.insert(
                    field.to_lowercase(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }
        // Legacy format: root.TemperatureControl.TemperatureSensor.0.Name=...
        else if let Some(rest) = key.strip_prefix("root.TemperatureControl.TemperatureSensor.") {
            if let Some((idx, field)) = rest.split_once('.') {
                let id = format!("S{}", idx);
                let entry = sensors.entry(id).or_default();
                match field {
                    "Value" => {
                        if let Ok(celsius) = value.parse::<f64>() {
                            entry.insert("celsius".to_string(), serde_json::json!((celsius * 10.0).round() / 10.0));
                            let fahrenheit = (celsius * 9.0 / 5.0) + 32.0;
                            entry.insert("fahrenheit".to_string(), serde_json::json!((fahrenheit * 10.0).round() / 10.0));
                        }
                    }
                    _ => {
                        entry.insert(
                            field.to_lowercase(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }
            }
        }
        // Legacy heater: root.TemperatureControl.Heater.0.Status=...
        else if let Some(rest) = key.strip_prefix("root.TemperatureControl.Heater.") {
            if let Some((idx, field)) = rest.split_once('.') {
                let id = format!("H{}", idx);
                let entry = heaters.entry(id.clone()).or_default();
                entry.insert("id".to_string(), serde_json::Value::String(id));
                entry.insert(
                    field.to_lowercase(),
                    serde_json::Value::String(value.to_string()),
                );
            }
        }
        // Other TemperatureControl entries
        else if let Some(field) = key.strip_prefix("root.TemperatureControl.") {
            extras.insert(
                field.to_lowercase(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    let sensor_list: Vec<_> = sensors.into_values().collect();
    let heater_list: Vec<_> = heaters.into_values().collect();

    let mut data = serde_json::Map::new();
    data.insert("sensors".to_string(), serde_json::json!(sensor_list));
    if !heater_list.is_empty() {
        data.insert("heaters".to_string(), serde_json::json!(heater_list));
    }
    for (k, v) in extras {
        data.insert(k, v);
    }

    serde_json::Value::Object(data)
}
