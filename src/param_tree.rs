use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display};

use chrono::NaiveDateTime;
use dxr::{TryFromValue, TryToValue, Value};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ParamTree {
    params: DashMap<String, InternalValue>,
    param_subscriptions: RwLock<Vec<ParamSubscription>>,
}

impl Display for ParamTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ParamTree:")?;
        for item in self.params.iter() {
            let key = item.key();
            let value = item.value();
            display_internal_value(value, key, f)?;
        }
        Ok(())
    }
}

fn display_internal_value(
    value: &InternalValue,
    prefix: &str,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match value {
        InternalValue::Integer(i) => writeln!(f, "{prefix}: {}", i)?,
        InternalValue::Boolean(b) => writeln!(f, "{prefix}: {}", b)?,
        InternalValue::String(s) => writeln!(f, "{prefix}: \"{}\"", s)?,
        InternalValue::Double(d) => writeln!(f, "{prefix}: {}", d)?,
        InternalValue::DateTime(d) => writeln!(f, "{prefix}: {}", d)?,
        InternalValue::Base64(b) => writeln!(f, "{prefix}: {:?}", b)?,
        InternalValue::Array(a) => {
            writeln!(f, "{prefix}: {:?}", a)?;
        }
        InternalValue::Structure(hm) => {
            for (key, value) in hm.iter() {
                display_internal_value(value, &format!("{prefix}/{key}"), f)?;
            }
        }
    }

    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
enum InternalValue {
    Integer(i32),
    Boolean(bool),
    String(String),
    Double(f64),
    DateTime(NaiveDateTime),
    Base64(Vec<u8>),
    Array(Vec<InternalValue>),
    Structure(HashMap<String, InternalValue>),
}

impl TryFromValue for InternalValue {
    fn try_from_value(value: &Value) -> Result<Self, dxr::DxrError> {
        let value = if let Ok(i) = i32::try_from_value(&value) {
            InternalValue::Integer(i)
        } else if let Ok(b) = bool::try_from_value(&value) {
            InternalValue::Boolean(b)
        } else if let Ok(s) = String::try_from_value(&value) {
            InternalValue::String(s)
        } else if let Ok(d) = f64::try_from_value(&value) {
            InternalValue::Double(d)
        } else if let Ok(d) = NaiveDateTime::try_from_value(&value) {
            InternalValue::DateTime(d)
        } else if let Ok(b) = Vec::<u8>::try_from_value(&value) {
            InternalValue::Base64(b)
        } else if let Ok(a) = Vec::<InternalValue>::try_from_value(&value) {
            InternalValue::Array(a)
        } else if let Ok(hm) = HashMap::<String, InternalValue>::try_from_value(&value) {
            InternalValue::Structure(hm)
        } else {
            log::error!("{:?}", value);
            return Err(dxr::DxrError::invalid_data(format!(
                "Failed to convert value: {:?}",
                value
            )));
        };

        Ok(value)
    }
}

impl TryToValue for InternalValue {
    fn try_to_value(&self) -> Result<Value, dxr::DxrError> {
        match self {
            InternalValue::Integer(i) => Ok(Value::i4(*i)),
            InternalValue::Boolean(b) => Ok(Value::boolean(*b)),
            InternalValue::String(s) => Ok(Value::string(s.clone())),
            InternalValue::Double(d) => Ok(Value::double(*d)),
            InternalValue::DateTime(d) => Ok(Value::string(d.to_string())),
            InternalValue::Base64(b) => Ok(Value::base64(b.to_vec())),
            InternalValue::Array(a) => Ok(a
                .iter()
                .map(|v| v.try_to_value().unwrap())
                .collect::<Vec<_>>()
                .try_to_value()
                .unwrap()),
            InternalValue::Structure(hm) => Ok(hm
                .iter()
                .map(|(k, v)| (k.clone(), v.try_to_value().unwrap()))
                .collect::<HashMap<_, _>>()
                .try_to_value()
                .unwrap()),
        }
    }
}

// impl From<Value> for InternalValue {
//     fn from(value: Value) -> Self {
// }

impl ParamTree {
    pub fn new() -> Self {
        Self {
            params: DashMap::new(),
            param_subscriptions: RwLock::new(Vec::new()),
        }
    }

    pub fn get_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();

        for item in self.params.iter() {
            let prefix = format!("/{}", item.key());

            keys.extend(self.get_keys_internal(&prefix, &item.value()));
        }
        keys
    }

    fn get_keys_internal(&self, prefix: &str, value: &InternalValue) -> Vec<String> {
        let mut keys = Vec::new();
        match value {
            InternalValue::Structure(hm) => {
                for (key, value) in hm.iter() {
                    let new_prefix = format!("{prefix}/{key}");
                    keys.extend(self.get_keys_internal(&new_prefix, value));
                }
            }
            _ => {
                keys.push(prefix.to_owned());
            }
        }
        keys
    }

    pub fn contains(&self, key: &str) -> Result<bool, String> {
        let key = key.trim_start_matches('/');
        if key == "" {
            return Ok(!self.params.is_empty());
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(value) = self.params.get(&key_prefix) {
                return Ok(self.contains_internal(&key_rest, &value));
            }
        } else {
            return Ok(self.params.contains_key(key));
        }

        Ok(false)
    }

    fn contains_internal(&self, key: &str, value: &InternalValue) -> bool {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match value {
                InternalValue::Structure(hm) => {
                    if let Some(value) = hm.get(&key_prefix) {
                        return self.contains_internal(&key_rest, &value);
                    } else {
                        return false;
                    }
                }
                _ => {
                    return false;
                }
            }
        } else {
            return true;
        }
    }

    pub fn set(&self, key: &str, value: Value) -> Result<(), String> {
        // We will take in a value and a key, and we will need to
        // 1. Convert the Value to a &[u8] slice, possibly using bincode
        // 2. Remove any existing subtrees that are children of the key
        // 3. Insert the key-value pair into the database
        // 4. Update the subscribers (ignore for now)
        let key = key.trim_start_matches('/');

        if key == "" {
            // root node so reset entire tree
            self.params.clear();

            if let Ok(value) = HashMap::<String, Value>::try_from_value(&value) {
                for (key, value) in value.into_iter() {
                    self.set(&key, value)?;
                }
            } else {
                return Err(format!("Root node must be a hashmap: {:?}", value));
            }

            return Ok(());
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(mut existing_value) = self.params.get_mut(&key_prefix) {
                self.insert_value(&key_rest, &mut existing_value, value)?;
            } else {
                let mut new_hm = InternalValue::Structure(HashMap::new());
                self.insert_value(&key_rest, &mut new_hm, value)?;
                self.params.insert(key_prefix, new_hm);
            }
        } else {
            self.params.insert(
                key.to_string(),
                InternalValue::try_from_value(&value).unwrap(),
            );
        }

        Ok(())
    }

    /// Recursively insert a value into the database.
    /// If the value is a HashMap, we will insert each leaf node into the database.
    /// If the value is not a HashMap, we will insert the value into the database.
    fn insert_value(
        &self,
        key: &str,
        existing_value: &mut InternalValue,
        value_to_insert: Value,
    ) -> Result<(), String> {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            // Check if existing value is a HashMap
            match existing_value {
                InternalValue::Structure(hm) => {
                    if let Some(mut internal_value) = hm.get_mut(&key_prefix) {
                        self.insert_value(&key_rest, &mut internal_value, value_to_insert)?;
                    } else {
                        let mut new_hm = InternalValue::Structure(HashMap::new());
                        self.insert_value(&key_rest, &mut new_hm, value_to_insert)?;
                        hm.insert(key_prefix, new_hm);
                    }
                }
                _ => {
                    let mut new_hm = InternalValue::Structure(HashMap::new());
                    self.insert_value(&key_rest, &mut new_hm, value_to_insert)?;
                    *existing_value = new_hm;
                }
            }
        } else {
            match existing_value {
                InternalValue::Structure(hm) => {
                    hm.insert(
                        key.to_string(),
                        InternalValue::try_from_value(&value_to_insert).unwrap(),
                    );
                }
                _ => {
                    let mut new_hm = HashMap::new();
                    new_hm.insert(
                        key.to_string(),
                        InternalValue::try_from_value(&value_to_insert).unwrap(),
                    );
                    *existing_value = InternalValue::Structure(new_hm);
                }
            }
        }

        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Value>, String> {
        // 1. We will need to get all values that have a prefix of the key
        // 2. We will need to decode the values
        // 3. We will need to convert those into a single Value
        let key = key.trim_start_matches('/');

        if key == "" {
            // root node so return entire tree as a hashmap
            let mut hm = HashMap::new();
            for item in self.params.iter() {
                hm.insert(item.key().clone(), item.value().try_to_value().unwrap());
            }
            return Ok(Some(hm.try_to_value().map_err(|e| e.to_string())?));
        }

        if let Some(value) = self.params.get(key) {
            return Ok(Some(value.try_to_value().map_err(|e| e.to_string())?));
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(value) = self.params.get(&key_prefix) {
                return self.get_internal(&key_rest, &value);
            }
        }

        return Ok(None);
    }

    fn get_internal(&self, key: &str, value: &InternalValue) -> Result<Option<Value>, String> {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match value {
                InternalValue::Structure(hm) => {
                    if let Some(value) = hm.get(&key_prefix) {
                        return self.get_internal(&key_rest, &value);
                    } else {
                        return Ok(None);
                    }
                }
                _ => {
                    return Ok(None);
                }
            }
        } else {
            match value {
                InternalValue::Structure(hm) => {
                    return Ok(Some(
                        hm.get(key)
                            .unwrap()
                            .try_to_value()
                            .map_err(|e| e.to_string())?,
                    ));
                }
                _ => {
                    return Ok(None);
                }
            }
        }
    }

    pub async fn delete(&self, key: &str) {
        // 1. We will need to delete all entries that have a prefix of the key
        let key = key.trim_start_matches('/');

        if let Some(_value) = self.params.get(key) {
            self.params.remove(key);
            return;
        }

        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            if let Some(mut value) = self.params.get_mut(&key_prefix) {
                self.delete_internal(&key_rest, &mut value);
            }
        }

        // self.update_subscribers(key, caller_id).await;
    }

    fn delete_internal(&self, key: &str, value: &mut InternalValue) -> Result<(), String> {
        if let Some((key_prefix, key_rest)) = key.split_once('/') {
            let key_prefix = key_prefix.to_string();
            let key_rest = key_rest.to_string();

            match value {
                InternalValue::Structure(hm) => {
                    if let Some(mut internal_value) = hm.get_mut(&key_prefix) {
                        return self.delete_internal(&key_rest, &mut internal_value);
                    } else {
                        return Ok(());
                    }
                }
                _ => {
                    return Ok(());
                }
            }
        } else {
            match value {
                InternalValue::Structure(hm) => {
                    hm.remove(key);
                    return Ok(());
                }
                _ => {
                    return Ok(());
                }
            }
        }
    }

    pub async fn subscribe(
        &self,
        node_id: String,
        param: String,
        api_uri: String,
    ) -> Result<Option<Value>, String> {
        {
            let mut param_subscriptions = self.param_subscriptions.write().await;
            let mut api_uri = Some(api_uri);

            // replace old entry if subscribing node has restarted
            for subscription in param_subscriptions.iter_mut() {
                if &subscription.node_id == &node_id && &subscription.param == &param {
                    subscription.api_uri = api_uri.take().unwrap();
                    break;
                }
            }

            // add a new entry if it's a new node id
            if let Some(api_uri) = api_uri {
                param_subscriptions.push(ParamSubscription {
                    node_id,
                    param: param.clone(),
                    api_uri,
                });
            }
        } // param_subscriptions lock released here

        self.get(&param)
    }

    /// Returns true if the subscription was removed, false if it was not found.
    pub async fn unsubscribe(&self, caller_api: String, key: String) -> bool {
        let mut param_subscriptions = self.param_subscriptions.write().await;
        let mut removed = false;
        param_subscriptions.retain(|subscription| {
            if subscription.api_uri == caller_api && subscription.param == key {
                removed = true;
                false
            } else {
                true
            }
        });
        removed
    }

    async fn update_subscribers(&self, key: &str, caller_id: String) {
        // let mut update_futures = JoinSet::new();
        // let param_subscriptions = self.param_subscriptions.read().await;

        // for subscription in param_subscriptions.iter() {
        //     if one_is_prefix_of_the_other(&key, &subscription.param) {
        //         let subscribed_key_spit = subscription
        //             .param
        //             .strip_prefix('/')
        //             .unwrap_or(&subscription.param)
        //             .split('/');

        //         if let Some(new_value) = self.params.read().await.get(subscribed_key_spit) {
        //             update_futures.spawn(update_client_with_new_param_value(
        //                 subscription.api_uri.clone(),
        //                 caller_id.clone(),
        //                 subscription.node_id.clone(),
        //                 subscription.param.clone(),
        //                 new_value,
        //             ));
        //         } else {
        //             log::warn!(
        //                 "Parameter {} no longer exists, skipping update for subscriber {}",
        //                 subscription.param,
        //                 subscription.node_id
        //             );
        //         }
        //     }
        // }

        // while let Some(res) = update_futures.join_next().await {
        //     match res {
        //         // Ok(Ok(v)) => {
        //         //     log::debug!("a subscriber has been updated (res: {:#?})", &v);
        //         // }
        //         // Ok(Err(err)) => {
        //         //     log::warn!(
        //         //         "Error updating a subscriber of changed param {}:\n{:#?}",
        //         //         &key,
        //         //         err
        //         //     );
        //         // }
        //         Err(err) => {
        //             log::warn!(
        //                 "Error updating a subscriber of changed param {}:\n{:#?}",
        //                 &key,
        //                 err
        //             );
        //         }
        //         _ => (),
        //     }
        // }
    }
}

/// Given a key `/example/key/value`, we want to create a hashmap with the following structure:
/// {
///     "example": {
///         "key": {
///             "value": Value
///         }
///     }
/// }
fn insert_value(key: &str, value: Value, hm: &mut HashMap<String, Value>) -> Result<(), String> {
    if let Some((key_next, key_rest)) = key.split_once('/') {
        if key_rest.is_empty() {
            hm.insert(key_next.to_string(), value);
            return Ok(());
        } else if let Some(next_hm) = hm.get_mut(key_next) {
            let mut hm_from_value =
                HashMap::<String, Value>::try_from_value(next_hm).map_err(|e| e.to_string())?;
            insert_value(key_rest, value, &mut hm_from_value)?;
            return Ok(());
        } else {
            let mut new_hm = HashMap::new();
            insert_value(key_rest, value, &mut new_hm)?;
            hm.insert(
                key_next.to_string(),
                new_hm.try_to_value().map_err(|e| e.to_string())?,
            );
            return Ok(());
        }
    } else {
        hm.insert(key.to_string(), value);
        return Ok(());
    }
}

#[derive(Debug)]
pub struct ParamSubscription {
    node_id: String,
    param: String,
    api_uri: String,
}

fn one_is_prefix_of_the_other(a: &str, b: &str) -> bool {
    let len = a.len().min(b.len());
    a[..len] == b[..len]
}

// async fn update_client_with_new_param_value(
//     client_api_url: String,
//     _updating_node_id: String,
//     _subscribing_node_id: String,
//     param_name: String,
//     new_value: ParamValue,
// ) -> Result<Value, anyhow::Error> {
//     let _client_api = ClientApi::new(&client_api_url);
//     let param_value = new_value
//         .try_to_value()
//         .map_err(|e| anyhow::anyhow!("Failed to convert param value: {}", e))?;

//     log::info!("paramUpdate[{}]", param_name);
//     // TODO: remove this once we have a way to test the param update
//     return Ok(param_value);

//     // let request = client_api.param_update(&updating_node_id, &param_name, &param_value);
//     // let res = request.await;
//     // match res {
//     //     Ok(ref v) => log::debug!(
//     //         "Sent new value for param '{}' to node '{}'. response: {:?}",
//     //         param_name,
//     //         subscribing_node_id,
//     //         &v
//     //     ),
//     //     Err(ref e) => log::debug!(
//     //         "Error sending new value for param '{}' to node '{}': {:?}",
//     //         param_name,
//     //         subscribing_node_id,
//     //         e
//     //     ),
//     // }

//     // Ok(res?)
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_value() {
        let mut hm = HashMap::new();
        insert_value(
            "example/key/value",
            Value::string("value".to_owned()),
            &mut hm,
        )
        .unwrap();
        let hm_value = hm.try_to_value().unwrap();

        assert_eq!(
            hm_value,
            HashMap::from([(
                "example".to_owned(),
                HashMap::from([(
                    "key".to_owned(),
                    HashMap::from([("value".to_owned(), Value::string("value".to_owned()))])
                )])
            )])
            .try_to_value()
            .unwrap()
        );
    }

    #[test]
    fn test_param_value() {
        let tree = ParamTree::new();
        tree.set("run_id", Value::string("asdf-jkl0".to_owned()))
            .unwrap();
        tree.set("robot_configs", Value::i4(23)).unwrap();
        tree.set("robot_configs/robot_speed", Value::double(3.0))
            .unwrap();
        tree.set("robot_configs/robot_id", Value::i4(24)).unwrap();
        tree.set("arms/arm_left/length", Value::double(-0.45))
            .unwrap();
        tree.set("arms/arm_right/length", Value::double(0.45))
            .unwrap();

        tree.set("robot_configs", Value::i4(23)).unwrap();
        let res: Value = tree.get("robot_configs").unwrap().unwrap();
        assert_eq!(res, Value::i4(23));

        println!("{}", tree);

        assert!(tree.contains("/").unwrap());
        assert!(tree.contains("/arms").unwrap());
        assert!(tree.contains("/arms/arm_left").unwrap());
    }

    #[test]
    fn test_param_tree_simple() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::new();
        tree.set("run_id", run_id.clone()).unwrap();

        // relative path or absolute path should work
        assert_eq!(tree.get("run_id").unwrap(), Some(run_id.clone()));
        assert_eq!(tree.get("/run_id").unwrap(), Some(run_id.clone()));
    }

    #[test]
    fn test_param_tree_set_get() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::new();
        tree.set("run_id", run_id.clone()).unwrap();

        let param_value = Value::string("param_value".to_owned());
        tree.set("some/param", param_value.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));
    }

    #[test]
    fn test_param_tree_set_get_array() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::new();
        tree.set("run_id", run_id.clone()).unwrap();

        let param_value = vec![
            Value::string("param_value".to_owned()),
            Value::string("param_value2".to_owned()),
        ]
        .try_to_value()
        .unwrap();

        tree.set("some/param", param_value.clone()).unwrap();

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));
    }

    #[test]
    fn test_param_tree_set_get_hashmap() {
        let tree = ParamTree::new();

        let param_value = HashMap::from([(
            "param_key".to_owned(),
            HashMap::from([(
                "param_key2".to_owned(),
                Value::string("param_value".to_owned()),
            )]),
        )])
        .try_to_value()
        .unwrap();

        tree.set("some/param", param_value.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").unwrap(), Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").unwrap(), Some(param_value.clone()));

        assert_eq!(
            tree.get("some/param/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
        assert_eq!(
            tree.get("/some/param/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
    }

    #[test]
    fn test_param_tree_set_get_hashmap_root() {
        let run_id = Value::string("therunid".to_owned());
        let tree = ParamTree::new();
        tree.set("run_id", run_id.clone()).unwrap();

        let param_tree = HashMap::from([(
            "param_key".to_owned(),
            HashMap::from([(
                "param_key2".to_owned(),
                Value::string("param_value".to_owned()),
            )]),
        )])
        .try_to_value()
        .unwrap();

        tree.set("/", param_tree.clone()).unwrap();

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("/").unwrap(), Some(param_tree.clone()));
        assert_eq!(
            tree.get("/param_key").unwrap(),
            Some(
                HashMap::from([(
                    "param_key2".to_owned(),
                    Value::string("param_value".to_owned()),
                )])
                .try_to_value()
                .unwrap()
            )
        );
        assert_eq!(
            tree.get("/param_key/param_key2").unwrap(),
            Some(Value::string("param_value".to_owned()))
        );
    }

    fn create_complex_value() -> Value {
        HashMap::from([
            ("robot_id".to_owned(), Value::i4(42)),
            (
                "robot_configs".to_owned(),
                HashMap::from([
                    ("robot_speed".to_owned(), Value::double(3.0)),
                    ("robot_id".to_owned(), Value::i4(24)),
                ])
                .try_to_value()
                .unwrap(),
            ),
            (
                "sim_001".to_owned(),
                HashMap::from([
                    (
                        "arms".to_owned(),
                        HashMap::from([
                            (
                                "arm_left".to_owned(),
                                HashMap::from([
                                    ("length".to_owned(), Value::double(-0.45)),
                                    (
                                        "joints".to_owned(),
                                        HashMap::from([
                                            ("shoulder".to_owned(), Value::double(0.0)),
                                            ("elbow".to_owned(), Value::double(90.0)),
                                            ("wrist".to_owned(), Value::double(45.0)),
                                        ])
                                        .try_to_value()
                                        .unwrap(),
                                    ),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                            (
                                "arm_right".to_owned(),
                                HashMap::from([
                                    ("length".to_owned(), Value::double(0.45)),
                                    ("status".to_owned(), Value::string("active".to_owned())),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                        ])
                        .try_to_value()
                        .unwrap(),
                    ),
                    (
                        "sensors".to_owned(),
                        HashMap::from([
                            (
                                "camera".to_owned(),
                                HashMap::from([
                                    (
                                        "resolution".to_owned(),
                                        vec![Value::i4(1920), Value::i4(1080)]
                                            .try_to_value()
                                            .unwrap(),
                                    ),
                                    ("fps".to_owned(), Value::i4(30)),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                            (
                                "lidar".to_owned(),
                                HashMap::from([
                                    ("range".to_owned(), Value::double(100.0)),
                                    ("frequency".to_owned(), Value::double(10.0)),
                                ])
                                .try_to_value()
                                .unwrap(),
                            ),
                        ])
                        .try_to_value()
                        .unwrap(),
                    ),
                ])
                .try_to_value()
                .unwrap(),
            ),
        ])
        .try_to_value()
        .unwrap()
    }

    fn load_state() -> ParamTree {
        let tree = ParamTree::new();
        tree.set("run_id", Value::string("asdf-jkl0".to_owned()))
            .unwrap();
        tree.set("/", create_complex_value()).unwrap();

        tree
    }

    #[test]
    fn test_param_tree_get_keys() {
        let tree = load_state();
        let keys = tree.get_keys();
        println!("{}", tree);

        assert!(keys.contains(&"/sim_001/arms/arm_left/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/status".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/resolution".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/fps".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/range".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/frequency".to_owned()));
    }
}
