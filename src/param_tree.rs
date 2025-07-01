use std::{collections::HashMap, fmt::Display, mem};

use dxr::{TryFromValue, TryToValue, Value};

#[derive(Debug)]
pub struct ParamTree {
    pub params: RwLock<ParamValue>,
    param_subscriptions: RwLock<Vec<ParamSubscription>>,
}

impl Display for ParamTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Ok(params) = self.params.try_read() {
            write!(f, "{}", params)
        } else {
            write!(f, "ParamTree(locked)")
        }
    }
}

impl Display for ParamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_with_path(f, "")
    }
}

impl ParamValue {
    fn fmt_with_path(&self, f: &mut std::fmt::Formatter<'_>, path: &str) -> std::fmt::Result {
        match self {
            ParamValue::HashMap(hm) => {
                for (k, v) in hm.iter() {
                    let new_path = if path.is_empty() {
                        k.to_string()
                    } else {
                        format!("{path}/{k}")
                    };
                    v.fmt_with_path(f, &new_path)?;
                }
                Ok(())
            }
            ParamValue::Array(arr) => {
                for (i, v) in arr.iter().enumerate() {
                    let new_path = format!("{path}[{i}]");
                    v.fmt_with_path(f, &new_path)?;
                }
                Ok(())
            }
            ParamValue::Value(v) => {
                writeln!(f, "{path}: {:?}", v)
            }
        }
    }
}

impl ParamTree {
    pub fn new(run_id: ParamValue) -> Self {
        Self {
            params: RwLock::new(ParamValue::HashMap(hashmap! {
                "run_id".to_owned() => run_id
            })),
            param_subscriptions: RwLock::new(Vec::new()),
        }
    }

    pub async fn get_keys(&self) -> Vec<String> {
        let params = self.params.read().await;
        params.get_keys()
    }

    pub async fn contains(&self, key: &str) -> bool {
        let params = self.params.read().await;
        params.contains(key)
    }

    pub async fn get(&self, key: &str) -> Option<ParamValue> {
        let key_path = key.strip_prefix('/').unwrap_or(&key).split('/');
        let params = self.params.read().await;
        params.get(key_path)
    }

    pub async fn set(&self, key: &str, value: ParamValue, caller_id: String) {
        if key == "/" {
            if matches!(value, ParamValue::HashMap(_)) {
                let mut params = self.params.write().await;
                let _ = mem::replace(&mut *params, value);
            } else {
                error!("tried to set root to non-hashmap");
            }
            return;
        }

        let key_path = key.strip_prefix('/').unwrap_or(&key).split('/');
        let mut params = self.params.write().await;
        params.update_inner(key_path, value);
        self.update_subscribers(key, caller_id).await;
    }

    pub async fn delete(&self, key: &str, caller_id: String) {
        let key_split = key.strip_prefix('/').unwrap_or(&key).split('/');
        let mut params = self.params.write().await;
        params.remove(key_split);
        self.update_subscribers(key, caller_id).await;
    }

    pub async fn subscribe(&self, node_id: String, param: String, api_uri: String) -> ParamValue {
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

        let value = self.params.read().await.get(param.split('/')).unwrap();
        value
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
        let mut update_futures = JoinSet::new();
        let param_subscriptions = self.param_subscriptions.read().await;
        for subscription in param_subscriptions.iter() {
            log::debug!(
                "subscriber {:?} has subscription? {}",
                &subscription,
                one_is_prefix_of_the_other(&key, &subscription.param)
            );
            if one_is_prefix_of_the_other(&key, &subscription.param) {
                let subscribed_key_spit = subscription
                    .param
                    .strip_prefix('/')
                    .unwrap_or(&subscription.param)
                    .split('/');

                let new_value = self.params.read().await.get(subscribed_key_spit).unwrap();
                update_futures.spawn(update_client_with_new_param_value(
                    subscription.api_uri.clone(),
                    caller_id.clone(),
                    subscription.node_id.clone(),
                    subscription.param.clone(),
                    new_value,
                ));
            }
        }

        while let Some(res) = update_futures.join_next().await {
            match res {
                Ok(Ok(v)) => {
                    log::debug!("a subscriber has been updated (res: {:#?})", &v);
                }
                Ok(Err(err)) => {
                    log::warn!(
                        "Error updating a subscriber of changed param {}:\n{:#?}",
                        &key,
                        err
                    );
                }
                Err(err) => {
                    log::warn!(
                        "Error updating a subscriber of changed param {}:\n{:#?}",
                        &key,
                        err
                    );
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct ParamSubscription {
    node_id: String,
    param: String,
    api_uri: String,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ParamValue {
    HashMap(HashMap<String, ParamValue>),
    Array(Vec<ParamValue>),
    Value(Value),
}

impl Into<ParamValue> for Value {
    fn into(self) -> ParamValue {
        ParamValue::from(&self)
    }
}

impl From<&Value> for ParamValue {
    fn from(value: &Value) -> Self {
        if let Ok(hm) = HashMap::<String, Value>::try_from_value(value) {
            let mut rv = HashMap::with_capacity(hm.len());
            for (k, v) in hm.into_iter() {
                rv.insert(k, ParamValue::from(&v));
            }
            return Self::HashMap(rv);
        }
        if let Ok(vec) = Vec::<Value>::try_from_value(value) {
            let mut rv = Vec::with_capacity(vec.len());
            for e in vec.into_iter() {
                rv.push(ParamValue::from(&e))
            }
            return Self::Array(rv);
        }
        Self::Value(value.clone())
    }
}

impl TryToValue for ParamValue {
    fn try_to_value(&self) -> Result<Value, dxr::DxrError> {
        match self {
            ParamValue::Value(v) => Ok(v.clone()),
            ParamValue::Array(arr) => arr.try_to_value(),
            ParamValue::HashMap(hm) => hm.try_to_value(),
        }
    }
}

impl ParamValue {
    pub fn get_keys(&self) -> Vec<String> {
        match self {
            ParamValue::HashMap(hm) => {
                let mut keys = Vec::new();
                for (k, v) in hm.iter() {
                    keys.push(format!("/{k}"));
                    for suffix in v.get_keys() {
                        keys.push(format!("/{k}{suffix}"));
                    }
                }
                keys
            }
            _ => Vec::new(),
        }
    }

    pub fn contains(&self, key: &str) -> bool {
        let key = key.split('/');
        self.get(key).is_some()
    }

    pub fn get<I, T>(&self, key: I) -> Option<ParamValue>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let mut hm = self;
        for e in key.into_iter() {
            let e = e.as_ref();
            if e == "" {
                continue;
            }
            match hm {
                ParamValue::HashMap(inner) => {
                    if let Some(inner_value) = inner.get(e) {
                        hm = inner_value;
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }

        Some(hm.clone())
    }

    pub fn remove<I, T>(&mut self, key: I)
    where
        I: IntoIterator<Item = T>,
        T: AsRef<str>,
    {
        let mut peekable = key.into_iter().peekable();
        match self {
            ParamValue::HashMap(inner) => {
                let mut hm = inner;
                loop {
                    let current_key = peekable.next();
                    let next_key = peekable.peek();
                    match (current_key, next_key) {
                        (Some(current_key), None) => {
                            hm.remove(current_key.as_ref());
                            return;
                        }
                        (None, None) => {
                            let _ = mem::replace(self, ParamValue::HashMap(hashmap! {}));
                            return;
                        }
                        (None, Some(_)) => unreachable!(),
                        (Some(current_key), Some(_)) => match hm.get_mut(current_key.as_ref()) {
                            Some(ParamValue::HashMap(new_hm)) => hm = new_hm,
                            _ => return,
                        },
                    }
                }
            }
            _ => (),
        }
    }

    pub fn update_inner<I, T>(&mut self, mut key: I, value: ParamValue)
    where
        I: Iterator<Item = T>,
        T: AsRef<str>,
    {
        match key.next() {
            None => {
                let _ = mem::replace(self, value);
            }
            Some(next_key) => match self {
                ParamValue::HashMap(hm) => match hm.get_mut(next_key.as_ref()) {
                    Some(inner) => inner.update_inner(key, value),
                    None => {
                        hm.insert(next_key.as_ref().to_string(), {
                            let mut inner = ParamValue::HashMap(HashMap::new());
                            inner.update_inner(key, value);
                            inner
                        });
                    }
                },
                _ => {
                    let mut inner = ParamValue::HashMap(hashmap! {});
                    inner.update_inner(key, value);
                    let outer = ParamValue::HashMap(hashmap! {
                        next_key.as_ref().to_string() => inner
                    });
                    let _ = mem::replace(self, outer);
                }
            },
        }
    }
}

fn one_is_prefix_of_the_other(a: &str, b: &str) -> bool {
    let len = a.len().min(b.len());
    a[..len] == b[..len]
}

async fn update_client_with_new_param_value(
    client_api_url: String,
    updating_node_id: String,
    subscribing_node_id: String,
    param_name: String,
    new_value: ParamValue,
) -> Result<Value, anyhow::Error> {
    let client_api = ClientApi::new(&client_api_url);
    let param_value = new_value.try_to_value().unwrap();
    let request = client_api.param_update(&updating_node_id, &param_name, &param_value);
    let res = request.await;
    match res {
        Ok(ref v) => log::debug!(
            "Sent new value for param '{}' to node '{}'. response: {:?}",
            param_name,
            subscribing_node_id,
            &v
        ),
        Err(ref e) => log::debug!(
            "Error sending new value for param '{}' to node '{}': {:?}",
            param_name,
            subscribing_node_id,
            e
        ),
    }

    Ok(res?)
}

use log::error;
use maplit::hashmap;
use tokio::{sync::RwLock, task::JoinSet};

use crate::client_api::ClientApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_param_value() {
        let mut tree = ParamValue::HashMap(hashmap! {
            "run_id".to_owned() => ParamValue::Value(Value::string("asdf-jkl0".to_owned())),
            "robot_id".to_owned() => ParamValue::Value(Value::i4(42)),
            "robot_configs".to_owned() => ParamValue::Array(vec![
                ParamValue::HashMap(hashmap! {
                    "robot_speed".to_owned() => ParamValue::Value(Value::double(3.0)),
                    "robot_id".to_owned() => ParamValue::Value(Value::i4(24))
                })
            ]),
            "arms".to_owned() => ParamValue::HashMap(hashmap! {
                "arm_left".to_owned() => ParamValue::HashMap(hashmap! {
                    "length".to_owned() => ParamValue::Value(Value::double(-0.45))
                })
            })
        });

        tree.update_inner(["robot_configs"].iter(), ParamValue::Value(Value::i4(23)));
        let res: Value = tree.get(["robot_configs"]).unwrap().try_to_value().unwrap();
        assert_eq!(res, Value::i4(23));

        assert!(tree.contains("/".to_owned()));
        assert!(tree.contains("/arms".to_owned()));
        assert!(tree.contains("/arms/arm_left".to_owned()));
    }

    #[tokio::test]
    async fn test_param_tree_simple() {
        let run_id = ParamValue::Value(Value::string("therunid".to_owned()));
        let tree = ParamTree::new(run_id.clone());

        // relative path or absolute path should work
        assert_eq!(tree.get("run_id").await, Some(run_id.clone()));
        assert_eq!(tree.get("/run_id").await, Some(run_id.clone()));
    }

    #[tokio::test]
    async fn test_param_tree_set_get() {
        let run_id = ParamValue::Value(Value::string("therunid".to_owned()));
        let tree = ParamTree::new(run_id.clone());

        let param_value = ParamValue::Value(Value::string("param_value".to_owned()));
        tree.set(
            "some/param".to_owned(),
            param_value.clone(),
            "caller_id".to_owned(),
        )
        .await;

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").await, Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").await, Some(param_value.clone()));
    }

    #[tokio::test]
    async fn test_param_tree_set_get_array() {
        let run_id = ParamValue::Value(Value::string("therunid".to_owned()));
        let tree = ParamTree::new(run_id.clone());

        let param_value = ParamValue::Array(vec![
            ParamValue::Value(Value::string("param_value".to_owned())),
            ParamValue::Value(Value::string("param_value2".to_owned())),
        ]);

        tree.set(
            "some/param".to_owned(),
            param_value.clone(),
            "caller_id".to_owned(),
        )
        .await;

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").await, Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").await, Some(param_value.clone()));
    }

    #[tokio::test]
    async fn test_param_tree_set_get_hashmap() {
        let run_id = ParamValue::Value(Value::string("therunid".to_owned()));
        let tree = ParamTree::new(run_id.clone());

        let param_value = ParamValue::HashMap(hashmap! {
            "param_key".to_owned() => ParamValue::HashMap(hashmap! {
                "param_key2".to_owned() => ParamValue::Value(Value::string("param_value".to_owned())),
            }),
        });

        tree.set(
            "some/param".to_owned(),
            param_value.clone(),
            "caller_id".to_owned(),
        )
        .await;

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("some/param").await, Some(param_value.clone()));
        assert_eq!(tree.get("/some/param").await, Some(param_value.clone()));

        assert_eq!(
            tree.get("some/param/param_key/param_key2").await,
            Some(ParamValue::Value(Value::string("param_value".to_owned())))
        );
        assert_eq!(
            tree.get("/some/param/param_key/param_key2").await,
            Some(ParamValue::Value(Value::string("param_value".to_owned())))
        );
    }

    #[tokio::test]
    async fn test_param_tree_set_get_hashmap_root() {
        let run_id = ParamValue::Value(Value::string("therunid".to_owned()));
        let tree = ParamTree::new(run_id.clone());

        let param_tree = ParamValue::HashMap(hashmap! {
            "param_key".to_owned() => ParamValue::HashMap(hashmap! {
                "param_key2".to_owned() => ParamValue::Value(Value::string("param_value".to_owned())),
            }),
        });

        tree.set("/".to_owned(), param_tree.clone(), "caller_id".to_owned())
            .await;

        println!("{}", tree);

        // relative path or absolute path should work
        assert_eq!(tree.get("/").await, Some(param_tree.clone()));
        assert_eq!(
            tree.get("/param_key").await,
            Some(ParamValue::HashMap(hashmap! {
                "param_key2".to_owned() => ParamValue::Value(Value::string("param_value".to_owned())),
            }))
        );
        assert_eq!(
            tree.get("/param_key/param_key2").await,
            Some(ParamValue::Value(Value::string("param_value".to_owned())))
        );
    }

    fn create_complex_tree() -> ParamValue {
        ParamValue::HashMap(hashmap! {
            "robot_id".to_owned() => ParamValue::Value(Value::i4(42)),
            "robot_configs".to_owned() => ParamValue::Array(vec![
                ParamValue::HashMap(hashmap! {
                    "robot_speed".to_owned() => ParamValue::Value(Value::double(3.0)),
                    "robot_id".to_owned() => ParamValue::Value(Value::i4(24))
                })
            ]),
            "sim_001".to_owned() => ParamValue::HashMap(hashmap! {
                "arms".to_owned() => ParamValue::HashMap(hashmap! {
                    "arm_left".to_owned() => ParamValue::HashMap(hashmap! {
                        "length".to_owned() => ParamValue::Value(Value::double(-0.45)),
                        "joints".to_owned() => ParamValue::HashMap(hashmap! {
                            "shoulder".to_owned() => ParamValue::Value(Value::double(0.0)),
                            "elbow".to_owned() => ParamValue::Value(Value::double(90.0)),
                            "wrist".to_owned() => ParamValue::Value(Value::double(45.0))
                        })
                    }),
                    "arm_right".to_owned() => ParamValue::HashMap(hashmap! {
                        "length".to_owned() => ParamValue::Value(Value::double(0.45)),
                        "status".to_owned() => ParamValue::Value(Value::string("active".to_owned()))
                    })
                }),
                "sensors".to_owned() => ParamValue::HashMap(hashmap! {
                    "camera".to_owned() => ParamValue::HashMap(hashmap! {
                        "resolution".to_owned() => ParamValue::Array(vec![
                            ParamValue::Value(Value::i4(1920)),
                            ParamValue::Value(Value::i4(1080))
                        ]),
                        "fps".to_owned() => ParamValue::Value(Value::i4(30))
                    }),
                    "lidar".to_owned() => ParamValue::HashMap(hashmap! {
                        "range".to_owned() => ParamValue::Value(Value::double(100.0)),
                        "frequency".to_owned() => ParamValue::Value(Value::double(10.0))
                    })
                })
            })
        })
    }
    async fn load_state() -> ParamTree {
        let tree = ParamTree::new(ParamValue::HashMap(hashmap! {
            "run_id".to_owned() => ParamValue::Value(Value::string("asdf-jkl0".to_owned())),
        }));
        tree.set("/".to_owned(), create_complex_tree(), "test".to_owned())
            .await;

        tree
    }

    #[tokio::test]
    async fn test_param_tree_get_keys() {
        let tree = load_state().await;
        let keys = tree.get_keys().await;
        println!("{:?}", keys);
        assert!(keys.contains(&"/sim_001/arms/arm_left/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/length".to_owned()));
        assert!(keys.contains(&"/sim_001/arms/arm_right/status".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/resolution".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/camera/fps".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/range".to_owned()));
        assert!(keys.contains(&"/sim_001/sensors/lidar/frequency".to_owned()));
    }
}
