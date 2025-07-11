use dxr::{TryFromValue, Value};
use serde_json;

/// Formats a `Value` object as valid JSON for logging purposes.
///
/// This function converts the `Value` to valid JSON format, properly escaping
/// special characters and using double quotes for strings. This makes the output
/// much easier to parse reliably.
///
/// # Arguments
///
/// * `value` - The `Value` object to format
///
/// # Returns
///
/// A JSON-formatted string representation of the value
pub fn format_value(value: &Value) -> String {
    // Convert the Value to serde_json::Value first, then serialize to JSON
    match value_to_serde_json(value) {
        Ok(json_value) => {
            serde_json::to_string(&json_value).unwrap_or_else(|_| format!("{:?}", value))
        }
        Err(_) => format!("{:?}", value),
    }
}

/// Converts a dxr::Value to serde_json::Value for JSON serialization
fn value_to_serde_json(value: &Value) -> Result<serde_json::Value, ()> {
    // Try to convert to various types in order of preference

    // Try string first
    if let Ok(s) = String::try_from_value(value) {
        return Ok(serde_json::Value::String(s));
    }

    // Try boolean
    if let Ok(b) = bool::try_from_value(value) {
        return Ok(serde_json::Value::Bool(b));
    }

    // Try i32
    if let Ok(i) = i32::try_from_value(value) {
        return Ok(serde_json::Value::Number(serde_json::Number::from(i)));
    }

    // Try f64
    if let Ok(f) = f64::try_from_value(value) {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return Ok(serde_json::Value::Number(n));
        }
    }

    // Try Vec<Value> (arrays)
    if let Ok(vec) = Vec::<Value>::try_from_value(value) {
        let mut json_array = Vec::new();
        for v in vec {
            if let Ok(json_val) = value_to_serde_json(&v) {
                json_array.push(json_val);
            }
        }
        return Ok(serde_json::Value::Array(json_array));
    }

    // Try HashMap<String, Value> (structs/objects)
    if let Ok(map) = std::collections::HashMap::<String, Value>::try_from_value(value) {
        let mut json_object = serde_json::Map::new();
        for (k, v) in map {
            if let Ok(json_val) = value_to_serde_json(&v) {
                json_object.insert(k, json_val);
            }
        }
        return Ok(serde_json::Value::Object(json_object));
    }

    // Try Vec<u8> (base64)
    if let Ok(bytes) = Vec::<u8>::try_from_value(value) {
        return Ok(serde_json::Value::String(format!(
            "<base64: {} bytes>",
            bytes.len()
        )));
    }

    // Try chrono::NaiveDateTime
    if let Ok(dt) = chrono::NaiveDateTime::try_from_value(value) {
        return Ok(serde_json::Value::String(format!("<datetime: {}>", dt)));
    }

    // If all else fails, return null
    Err(())
}

/// Formats a slice of `Value` objects as a comma-separated string using `format_value`.
///
/// # Arguments
///
/// * `params` - The slice of `Value` objects to format
///
/// # Returns
///
/// A formatted string representation of the parameters
pub fn format_params(params: &[Value]) -> String {
    params
        .iter()
        .map(format_value)
        .collect::<Vec<_>>()
        .join(",")
}

/// Formats a topic list for debug output in JSON format.
///
/// This function takes a vector of (topic_name, topic_type) tuples and formats them
/// as JSON arrays for better log parsing.
///
/// # Arguments
///
/// * `topics` - Vector of (String, String) tuples representing (topic_name, topic_type)
///
/// # Returns
///
/// A JSON-formatted string representation of the topics
pub fn format_topic_list(topics: &[(String, String)]) -> String {
    let topic_arrays: Vec<String> = topics
        .iter()
        .map(|(name, topic_type)| format!("[\"{}\", \"{}\"]", name, topic_type))
        .collect();
    format!("[{}]", topic_arrays.join(", "))
}

/// Formats a return value tuple for debug output in valid JSON format.
///
/// This function takes a tuple of (status, message, value) and formats it
/// as a JSON array for better log parsing.
///
/// # Arguments
///
/// * `status` - Integer status code
/// * `message` - String message
/// * `value` - The return value (can be any type)
///
/// # Returns
///
/// A JSON-formatted string representation of the return value as an array
pub fn format_return_value(status: i32, message: &str, value: &Value) -> String {
    let value_json = format_value(value);
    let message_json = serde_json::to_string(message).unwrap();
    format!("[{}, {}, {}]", status, message_json, value_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dxr::TryToValue;
    use maplit::hashmap;

    #[test]
    fn test_format_string() {
        let value = Value::string("hello world".to_string());
        assert_eq!(format_value(&value), "\"hello world\"");
    }

    #[test]
    fn test_format_boolean() {
        let value = Value::boolean(true);
        assert_eq!(format_value(&value), "true");

        let value = Value::boolean(false);
        assert_eq!(format_value(&value), "false");
    }

    #[test]
    fn test_format_integer() {
        let value = Value::i4(42);
        assert_eq!(format_value(&value), "42");

        let value = Value::i4(-123);
        assert_eq!(format_value(&value), "-123");
    }

    #[test]
    fn test_format_double() {
        let value = Value::double(3.14);
        assert_eq!(format_value(&value), "3.14");

        let value = Value::double(-2.5);
        assert_eq!(format_value(&value), "-2.5");
    }

    #[test]
    fn test_format_array() {
        let value = vec![
            Value::string("hello".to_string()),
            Value::i4(42),
            Value::boolean(true),
        ]
        .try_to_value()
        .unwrap();

        assert_eq!(format_value(&value), "[\"hello\",42,true]");
    }

    #[test]
    fn test_format_struct() {
        let value = hashmap! {
            "name".to_string() => Value::string("test".to_string()),
            "count".to_string() => Value::i4(42),
            "active".to_string() => Value::boolean(true),
        }
        .try_to_value()
        .unwrap();

        // Note: HashMap iteration order is not guaranteed, so we need to be flexible
        let formatted = format_value(&value);
        assert!(formatted.contains("\"name\":\"test\""));
        assert!(formatted.contains("\"count\":42"));
        assert!(formatted.contains("\"active\":true"));
        assert!(formatted.starts_with("{"));
        assert!(formatted.ends_with("}"));
    }

    #[test]
    fn test_format_nested() {
        let value = hashmap! {
            "user".to_string() => hashmap! {
                "name".to_string() => Value::string("alice".to_string()),
                "scores".to_string() => vec![
                    Value::i4(95),
                    Value::i4(87),
                    Value::i4(92),
                ].try_to_value().unwrap(),
            }.try_to_value().unwrap(),
            "active".to_string() => Value::boolean(true),
        }
        .try_to_value()
        .unwrap();

        let formatted = format_value(&value);
        assert!(formatted.contains("\"user\":{"));
        assert!(formatted.contains("\"name\":\"alice\""));
        assert!(formatted.contains("\"scores\":[95,87,92]"));
        assert!(formatted.contains("\"active\":true"));
    }

    #[test]
    fn test_format_params() {
        let params = vec![
            Value::string("foo".to_string()),
            Value::i4(42),
            Value::boolean(false),
        ];
        assert_eq!(format_params(&params), "\"foo\",42,false");
    }

    #[test]
    fn test_complex_value() {
        // [2025-07-09T12:23:40Z DEBUG ros_core_rs::core] registerPublisher[Value { value: String("/ln_rst_test_node") }, Value { value: String("/rosout") }, Value { value: String("rosgraph_msgs/Log") }, Value { value: String("http://LOCLAP858:41145/") }]
        let value = vec![
            "/ln_rst_test_node".to_string(),
            "/rosout".to_string(),
            "rosgraph_msgs/Log".to_string(),
            "http://LOCLAP858:41145/".to_string(),
        ]
        .try_to_value()
        .unwrap();

        assert_eq!(
            format_value(&value),
            "[\"/ln_rst_test_node\",\"/rosout\",\"rosgraph_msgs/Log\",\"http://LOCLAP858:41145/\"]"
        );
    }

    #[test]
    fn test_escaped_strings() {
        let value = Value::string("hello \"world\" with quotes".to_string());
        assert_eq!(format_value(&value), "\"hello \\\"world\\\" with quotes\"");
    }

    #[test]
    fn test_newlines_in_strings() {
        let value = Value::string("hello\nworld".to_string());
        assert_eq!(format_value(&value), "\"hello\\nworld\"");
    }
}
