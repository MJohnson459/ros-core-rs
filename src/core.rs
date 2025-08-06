use dxr_client::{Client, ClientBuilder, Url};
use paste::paste;
use std::collections::HashMap;
use std::sync::Arc;

use dxr_server::{async_trait, Handler, HandlerResult};
use dxr_server::{
    axum::{self, http::HeaderMap},
    RouteBuilder, Server,
};

use dxr::{DxrError, TryFromParams, TryFromValue, TryToValue, Value};

use crate::{
    master_state::MasterState,
    param_tree::ParamTree,
    utils::{format_value, param_to_string, CommonResponse},
};

/// An enum that represents the different types of endpoints that can be accessed in the ROS Master API.
///
/// # Variants
///
/// * `RegisterService`: Registers a service with the ROS Master.
/// * `UnRegisterService`: Unregisters a service with the ROS Master.
/// * `RegisterSubscriber`: Registers a subscriber with the ROS Master.
/// * `UnregisterSubscriber`: Unregisters a subscriber with the ROS Master.
/// * `RegisterPublisher`: Registers a publisher with the ROS Master.
/// * `UnregisterPublisher`: Unregisters a publisher with the ROS Master.
/// * `LookupNode`: Looks up a node with the ROS Master.
/// * `GetPublishedTopics`: Gets the published topics from the ROS Master.
/// * `GetTopicTypes`: Gets the topic types from the ROS Master.
/// * `GetSystemState`: Gets the system state from the ROS Master.
/// * `GetUri`: Gets the URI from the ROS Master.
/// * `LookupService`: Looks up a service with the ROS Master.
/// * `DeleteParam`: Deletes a parameter from the ROS Parameter Server.
/// * `SetParam`: Sets a parameter on the ROS Parameter Server.
/// * `GetParam`: Gets a parameter from the ROS Parameter Server.
/// * `SearchParam`: Searches for a parameter on the ROS Parameter Server.
/// * `SubscribeParam`: Subscribes to a parameter on the ROS Parameter Server.
/// * `UnsubscribeParam`: Unsubscribes from a parameter on the ROS Parameter Server.
/// * `HasParam`: Checks if a parameter exists on the ROS Parameter Server.
/// * `GetParamNames`: Gets the names of parameters on the ROS Parameter Server.
/// * `SystemMultiCall`: Performs multiple ROS Master API calls in a single request.
/// * `Default`: The default endpoint used when no other endpoint is specified.
enum MasterEndpoints {
    RegisterService,
    UnRegisterService,
    RegisterSubscriber,
    UnregisterSubscriber,
    RegisterPublisher,
    UnregisterPublisher,
    LookupNode,
    GetPublishedTopics,
    GetTopicTypes,
    GetSystemState,
    GetUri,
    LookupService,
    DeleteParam,
    SetParam,
    GetParam,
    SearchParam,
    SubscribeParam,
    UnsubscribeParam,
    HasParam,
    GetParamNames,
    SystemMultiCall,
    GetPid,
    Default,
}

impl MasterEndpoints {
    fn as_str(&self) -> &'static str {
        match self {
            MasterEndpoints::RegisterService => "registerService",
            MasterEndpoints::UnRegisterService => "unregisterService",
            MasterEndpoints::RegisterSubscriber => "registerSubscriber",
            MasterEndpoints::UnregisterSubscriber => "unregisterSubscriber",
            MasterEndpoints::RegisterPublisher => "registerPublisher",
            MasterEndpoints::UnregisterPublisher => "unregisterPublisher",
            MasterEndpoints::LookupNode => "lookupNode",
            MasterEndpoints::GetPublishedTopics => "getPublishedTopics",
            MasterEndpoints::GetTopicTypes => "getTopicTypes",
            MasterEndpoints::GetSystemState => "getSystemState",
            MasterEndpoints::GetUri => "getUri",
            MasterEndpoints::LookupService => "lookupService",
            MasterEndpoints::DeleteParam => "deleteParam",
            MasterEndpoints::SetParam => "setParam",
            MasterEndpoints::GetParam => "getParam",
            MasterEndpoints::SearchParam => "searchParam",
            MasterEndpoints::SubscribeParam => "subscribeParam",
            MasterEndpoints::UnsubscribeParam => "unsubscribeParam",
            MasterEndpoints::HasParam => "hasParam",
            MasterEndpoints::GetParamNames => "getParamNames",
            MasterEndpoints::SystemMultiCall => "system.multicall",
            MasterEndpoints::GetPid => "getPid",
            MasterEndpoints::Default => "",
        }
    }
}

/// Struct containing information about ROS data.
pub struct RosData {
    /// The master state, which contains information about the ROS network.
    master_state: MasterState,
    /// The global parameter tree.
    parameters: ParamTree,
    /// The URI to access this ROS master.
    uri: String,
}

pub struct Master {
    data: Arc<RosData>,
}

/// Handler for registering the caller as a provider of the specified service.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `service` - Fully-qualified name of service (string)
/// - `service_api` - ROSRPC Service URI (string)
/// - `caller_api` - XML-RPC URI of caller node (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `ignore` - ignore (integer)
struct RegisterServiceHandler {
    data: Arc<RosData>,
}
type RegisterServiceResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for RegisterServiceHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "registerService", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("registerService[{params_str}]");
        type Request = (String, String, String, String);
        let (caller_id, service, service_api, caller_api) = Request::try_from_params(params)?;

        // Check for empty service parameter
        if service.trim().is_empty() {
            let result = RegisterServiceResponse::new(
                -1,
                "ERROR: parameter [service] must be a non-empty string",
                0,
            );
            log::debug!("registerService[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        self.data
            .master_state
            .register_node(&caller_id, &caller_api)
            .await;

        self.data
            .master_state
            .register_service(&caller_id, &service, &service_api);

        let result = RegisterServiceResponse::new(
            1,
            format!("Registered [{caller_id}] as provider of [{service}]"),
            1,
        );
        log::info!("+SERVICE [{}] {} {}", service, caller_id, caller_api);
        log::debug!("registerService[{params_str}] returns [{result}]");

        Ok(result.try_to_value()?)
    }
}

/// Handler for unregistering the caller as a provider of the specified service.
///
/// # Parameters
///
/// - caller_id - ROS caller ID (string)
/// - service - Fully-qualified name of service (string)
/// - service_api - API URI of service to unregister. Unregistration will only occur if current
/// registration matches. (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - code - response code (integer)
/// - statusMessage - status message (string)
/// - numUnregistered - number of unregistrations (either 0 or 1). If this is zero it means that the
/// caller was not registered as a service provider. The call still succeeds as the intended final
/// state is reached. (integer)
struct UnRegisterServiceHandler {
    data: Arc<RosData>,
}
type UnRegisterServiceResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for UnRegisterServiceHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "unregisterService", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("unregisterService[{params_str}]");

        type Request = (String, String, String);
        let (caller_id, service, service_api) = Request::try_from_params(params)?;

        // Check for empty service parameter
        if service.trim().is_empty() {
            let result = UnRegisterServiceResponse::new(
                -1,
                "ERROR: parameter [service] must be a non-empty string",
                0,
            );
            log::debug!("unregisterService[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        let removed = self
            .data
            .master_state
            .unregister_service(&caller_id, &service, &service_api);

        let status = if removed {
            format!("Unregistered [{caller_id}] as provider of [{service}]")
        } else {
            format!("[{caller_id}] is not a registered node")
        };
        let result = UnRegisterServiceResponse::new(1, status.clone(), if removed { 1 } else { 0 });
        log::debug!("unregisterService[{params_str}] returns [{result}]");
        Ok(result.try_to_value()?)
    }
}

/// Handler for registering the caller as a subscriber to the specified topic.
///
/// # Parameters
///
/// - caller_id - ROS caller ID (string)
/// - topic - Fully-qualified name of the topic (string)
/// - topic_type - Datatype for topic. Must be a package-resource name, i.e. the .msg name (string)
/// - caller_api - API URI of subscriber to register. Will be used for new publisher notifications (string)
///
/// # Returns
///
/// A tuple of integers and a vector of strings representing the response:
///
/// - code - response code (integer)
/// - statusMessage - status message (string)
/// - publishers - a list of XMLRPC API URIs for nodes currently publishing the specified topic (vector of strings).
struct RegisterSubscriberHandler {
    data: Arc<RosData>,
}
type RegisterSubscriberResponse = CommonResponse<Vec<String>>;
#[async_trait]
impl Handler for RegisterSubscriberHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "registerSubscriber", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("registerSubscriber[{params_str}]");
        type Request = (String, String, String, String);
        let (caller_id, topic, topic_type, caller_api) = Request::try_from_params(params)?;

        // Check for empty topic parameter
        if topic.trim().is_empty() {
            let result = RegisterSubscriberResponse::new(
                -1,
                "ERROR: parameter [topic] must be a non-empty string",
                Vec::<String>::new(),
            );
            log::debug!("registerSubscriber[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        self.data
            .master_state
            .register_node(&caller_id, &caller_api)
            .await;

        let publisher_apis =
            self.data
                .master_state
                .register_subscriber(&caller_id, &topic, &topic_type);

        let result =
            RegisterSubscriberResponse::new(1, format!("Subscribed to [{topic}]"), publisher_apis);
        log::info!("+SUB [{}] {} {}", topic, caller_id, caller_api);
        log::debug!("registerSubscriber[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for unregistering the caller as a publisher of the topic.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `topic` - Fully-qualified name of topic (string)
/// - `caller_api` - API URI of subscriber to unregister. Unregistration will only occur if current
///   registration matches. (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `numUnsubscribed` - number of unsubscriptions (either 0 or 1). If this is zero it means that the caller was not
/// registered as a subscriber. The call still succeeds as the intended final state is reached.
struct UnRegisterSubscriberHandler {
    data: Arc<RosData>,
}
type UnRegisterSubscriberResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for UnRegisterSubscriberHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "unregisterSubscriber", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("unregisterSubscriber[{params_str}]");
        type Request = (String, String, String);
        let (caller_id, topic, caller_api) = Request::try_from_params(params)?;

        // Check for empty topic parameter
        if topic.trim().is_empty() {
            let result = UnRegisterSubscriberResponse::new(
                -1,
                "ERROR: parameter [topic] must be a non-empty string",
                0,
            );
            log::debug!("unregisterSubscriber[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        let removed = self
            .data
            .master_state
            .unregister_subscriber(&caller_id, &topic, &caller_api);

        let status = format!("Unregistered [{caller_id}] as provider of [{topic}]");
        let result = UnRegisterSubscriberResponse::new(1, status, if removed { 1 } else { 0 });
        log::debug!("unregisterSubscriber[{params_str}] returns [{result}]");
        Ok(result.try_to_value()?)
    }
}

/// Handler for registering the caller as a publisher of the specified topic.
///
/// # Parameters
///
/// - caller_id - ROS caller ID (string)
/// - topic - Fully-qualified name of topic to register (string)
/// - topic_type - Datatype for topic. Must be a package-resource name, i.e. the .msg name (string)
/// - caller_api - API URI of publisher to register (string)
///
/// # Returns
///
/// A tuple of integers, a string, and a list of strings representing the response:
///
/// - code - response code (integer)
/// - statusMessage - status message (string)
/// - subscriberApis - list of current subscribers of topic in the form of XMLRPC URIs (list of strings)
struct RegisterPublisherHandler {
    data: Arc<RosData>,
}
type RegisterPublisherResponse = CommonResponse<Vec<String>>;
#[async_trait]
impl Handler for RegisterPublisherHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "registerPublisher", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("registerPublisher[{params_str}]");
        type Request = (String, String, String, String);
        let (caller_id, topic, topic_type, caller_api) = Request::try_from_params(params)?;

        // Check for empty topic parameter
        if topic.trim().is_empty() {
            let result = (
                -1,
                "ERROR: parameter [topic] must be a non-empty string",
                Vec::<String>::new(),
            );
            log::debug!("registerPublisher[{params_str}] returns {result:?}");
            return Ok(result.try_to_value()?);
        }

        self.data
            .master_state
            .register_node(&caller_id, &caller_api)
            .await;

        let subscribers_api_urls =
            self.data
                .master_state
                .register_publisher(&caller_id, &topic, &topic_type);

        // Print early so the logs are in order.
        let result = RegisterPublisherResponse::new(
            1,
            format!("Registered [{caller_id}] as publisher of [{topic}]"),
            subscribers_api_urls.clone(),
        );
        log::info!("+PUB [{}] {} {}", topic, caller_id, caller_api);
        log::debug!("registerPublisher[{params_str}] returns [{result}]");

        // TODO(mj): This should be done in a background task.
        self.data
            .master_state
            .publisher_update(&caller_id, &topic, &subscribers_api_urls)
            .await;

        return Ok(result.try_to_value()?);
    }
}

/// Handler for unregistering the caller as a publisher of the topic.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `topic` - Fully-qualified name of topic to unregister (string)
/// - `caller_api` - API URI of publisher to unregister (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `numUnregistered` - number of unregistrations (either 0 or 1). If this is zero it means that the
/// caller was not registered as a publisher. The call still succeeds as the intended final state is reached.
struct UnRegisterPublisherHandler {
    data: Arc<RosData>,
}
type UnRegisterPublisherResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for UnRegisterPublisherHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "unregisterPublisher", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("unregisterPublisher[{params_str}]");
        type Request = (String, String, String);
        let (caller_id, topic, caller_api) = Request::try_from_params(params)?;

        // Check for empty topic parameter
        if topic.trim().is_empty() {
            let result = UnRegisterPublisherResponse::new(
                -1,
                "ERROR: parameter [topic] must be a non-empty string",
                0,
            );
            log::debug!("unregisterPublisher[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        let removed = self
            .data
            .master_state
            .unregister_publisher(&caller_id, &topic, &caller_api);

        let result = match removed {
            Ok(true) => UnRegisterPublisherResponse::new(
                1,
                format!("Unregistered [{caller_id}] as provider of [{topic}]"),
                1,
            ),
            Ok(false) => UnRegisterPublisherResponse::new(
                1,
                format!("[{caller_id}] is not a registered node"),
                0,
            ),
            Err(e) => UnRegisterPublisherResponse::new(-1, e, 0),
        };

        log::debug!("unregisterPublisher[{params_str}] returns [{result}]");
        Ok(result.try_to_value()?)
    }
}

/// Handler for looking up the XML-RPC URI of the node with the associated name/caller_id.
///
/// # Parameters
///
/// - caller_id - ROS caller ID (string)
/// - node_name - Name of node to lookup (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - code - response code (integer)
/// - statusMessage - status message (string)
/// - URI - XML-RPC URI of the node (string). This API is for looking up information about publishers
/// and subscribers. Use lookupService instead to lookup ROS-RPC URIs.
struct LookupNodeHandler {
    data: Arc<RosData>,
}
type LookupNodeResponse = CommonResponse<String>;
#[async_trait]
impl Handler for LookupNodeHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "lookupNode", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = params
            .iter()
            .map(|v| format_value(v))
            .collect::<Vec<_>>()
            .join(", ");
        log::debug!("lookupNode[{params_str}]");
        type Request = (String, String);
        let (_caller_id, node_name) = Request::try_from_params(params)?;

        // Check for empty node parameter
        if node_name.trim().is_empty() {
            let result = LookupNodeResponse::new(
                -1,
                "ERROR: parameter [node] must be a non-empty string",
                String::new(),
            );
            log::debug!("lookupNode[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        let node_api = self.data.master_state.lookup_node(&node_name);

        let result = if let Some(node_api) = node_api {
            LookupNodeResponse::new(1, String::new(), node_api)
        } else {
            LookupNodeResponse::new(-1, format!("unknown node [{}]", node_name), String::new())
        };

        log::debug!("lookupNode[{params_str}] returns [{result}]");
        Ok(result.try_to_value()?)
    }
}

/// Handler for getting the list of topics that can be subscribed to.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `subgraph` - Restrict topic names to match within the specified subgraph. Subgraph namespace
///   is resolved relative to the caller's namespace. Use empty string to specify all names (string).
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `topics` - a list of lists containing topic names and types, e.g. `[[topic1, type1], [topic2, type2]]`.
/// The list represents topics that can be subscribed to, but not necessarily all topics available in the system.
/// Use `getSystemState()` for a more comprehensive list.
struct GetPublishedTopicsHandler {
    data: Arc<RosData>,
}
type GetPublishedTopicsResponse = CommonResponse<Vec<(String, String)>>;
#[async_trait]
impl Handler for GetPublishedTopicsHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "getPublishedTopics", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getPublishedTopics[{params_str}]");
        type Request = (String, String);
        let (caller_id, subgraph) = Request::try_from_params(params)?;

        let topics = self
            .data
            .master_state
            .get_published_topics(&caller_id, &subgraph);
        let result = GetPublishedTopicsResponse::new(1, "current topics", topics);
        log::debug!("getPublishedTopics[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for retrieving the list of topic names and their types.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
///
/// # Returns
///
/// A tuple of integers, a string representing the response, and a list of lists of strings:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `topicTypes` - a list of lists of strings representing the [topicName, topicType] pairs.
struct GetTopicTypesHandler {
    data: Arc<RosData>,
}
type GetTopicTypesResponse = CommonResponse<Vec<(String, String)>>;
#[async_trait]
impl Handler for GetTopicTypesHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "getTopicTypes", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getTopicTypes[{params_str}]");
        type Request = String;
        let _caller_id = Request::try_from_params(params)?;
        let topics = self.data.master_state.get_topic_types();
        let result = GetTopicTypesResponse::new(1, "current system state", topics);
        log::debug!("getTopicTypes[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for retrieving a list representation of the ROS system state.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `systemState` - a list of tuples representing the system state:
///     - Each tuple contains two elements: a string representing the topic/service name, and a list of strings
///       representing the associated publishers/subscribers/servers/clients.
///     - The first tuple contains the list of publishers, the second contains the list of subscribers, and the third
///       contains the list of services.
struct GetSystemStateHandler {
    data: Arc<RosData>,
}
type GetSystemStateResponse = CommonResponse<(
    Vec<(String, Vec<String>)>,
    Vec<(String, Vec<String>)>,
    Vec<(String, Vec<String>)>,
)>;
#[async_trait]
impl Handler for GetSystemStateHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "getSystemState", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = params
            .iter()
            .map(|v| format_value(v))
            .collect::<Vec<_>>()
            .join(", ");
        log::debug!("getSystemState[{params_str}]");
        type Request = String;
        let _caller_id = Request::try_from_params(params)?;

        let (publishers, subscribers, services) = self.data.master_state.get_system_state();
        let publishers_clone = publishers.clone();
        let subscribers_clone = subscribers.clone();
        let services_clone = services.clone();
        let result = GetSystemStateResponse::new(
            1,
            "",
            (publishers_clone, subscribers_clone, services_clone),
        );
        log::debug!("getSystemState[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for getting the URI of the master.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `masterURI` - URI of the ROS master (string)
struct GetUriHandler {
    data: Arc<RosData>,
}
type GetUriResponse = CommonResponse<String>;
#[async_trait]
impl Handler for GetUriHandler {
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "getUri", skip_all))]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getUri[{params_str}]");

        let result = GetUriResponse::new(1, "", self.data.uri.clone());
        log::debug!("getUri[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for getting the PID of the master.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `pid` - PID of the ROS master (integer)
struct GetPidHandler {
    #[allow(unused)]
    data: Arc<RosData>,
}
type GetPidResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for GetPidHandler {
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "getPid", skip_all))]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getPid[{params_str}]");
        type Request = String;
        let _caller_id = Request::try_from_params(params)?;
        let result = std::process::id() as i32; // max pid on linux is 2^22, so the typecast should have no unintended side effects
        let result = GetPidResponse::new(1, "", result);
        log::debug!("getPid[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for looking up all providers of a particular service.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `service` - Fully-qualified name of service (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `serviceUrl` - URL that provides the address and port of the service. The function fails if there
/// is no provider.
struct LookupServiceHandler {
    data: Arc<RosData>,
}
type LookupServiceResponse = CommonResponse<String>;
#[async_trait]
impl Handler for LookupServiceHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "lookupService", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("lookupService[{params_str}]");
        type Request = (String, String);
        let (caller_id, service) = Request::try_from_params(params)?;

        // Check for empty service parameter
        if service.trim().is_empty() {
            let result = LookupServiceResponse::new(
                -1,
                "ERROR: parameter [service] must be a non-empty string".to_string(),
                "".to_string(),
            );
            log::debug!("lookupService[{params_str}] returns [{result}]");
            return Ok(result.try_to_value()?);
        }

        let service_url = self.data.master_state.lookup_service(&caller_id, &service);

        let result = match service_url {
            Ok(service_url) => LookupServiceResponse::new(1, String::new(), service_url),
            Err(e) => LookupServiceResponse::new(-1, e, String::new()),
        };

        log::debug!("lookupService[{params_str}] returns [{result}]");
        return Ok(result.try_to_value()?);
    }
}

/// Handler for deleting a parameter.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `key` - Parameter name (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `ignore` - an integer indicating the number of parameters deleted. This is always 0, since a delete
/// operation deletes only one parameter.
struct DeleteParamHandler {
    data: Arc<RosData>,
}
type DeleteParamResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for DeleteParamHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "deleteParam", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("deleteParam[{params_str}]");
        type Request = (String, String);
        let (caller_id, key) = Request::try_from_params(params)?;
        let key = resolve(&caller_id, &key);
        let deleted = self.data.parameters.delete(&key);

        // Print early so the logs are in order.
        let result = if deleted {
            DeleteParamResponse::new(1, format!("parameter [{key}] deleted"), 0)
        } else {
            DeleteParamResponse::new(-1, format!("parameter [{key}] is not set"), 0)
        };

        log::debug!("deleteParam[{params_str}] returns [{result}]");

        self.data
            .parameters
            .update_subscribers(&key, caller_id.clone())
            .await;

        return Ok(result.try_to_value()?);
    }
}

/// Handler for setting a ROS parameter.
///
/// # Parameters
///
/// - caller_id - ROS caller ID (string)
/// - key - Parameter name (string)
/// - value - Parameter value. If it's a dictionary, it will be treated as a parameter tree, where
/// the key is the parameter namespace. For example {'x':1,'y':2,'sub':{'z':3}} will set
/// key/x=1, key/y=2, and key/sub/z=3. Furthermore, it will replace all existing parameters
/// in the key parameter namespace with the parameters in value. You must set parameters individually
/// if you wish to perform a union update (XMLRPCLegalValue)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - code - response code (integer)
/// - statusMessage - status message (string)
/// - ignore - ignored (integer). Returns 0 in all cases.
struct SetParamHandler {
    data: Arc<RosData>,
}
type SetParamResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for SetParamHandler {
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "setParam", skip_all))]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("setParam[{params_str}]");
        type Request = (String, String, Value);
        let (caller_id, key, value) = Request::try_from_params(params)?;
        let key = resolve(&caller_id, &key);

        let status = self.data.parameters.set(&key, value);

        // Print early so the logs are in order.
        let result = match status {
            Ok(_) => {
                log::info!("+PARAM [{key}] by {caller_id}");
                SetParamResponse::new(1, format!("parameter {key} set"), 0)
            }
            Err(e) => SetParamResponse::new(-1, format!("Error: {e}"), 0),
        };

        log::debug!("setParam[{params_str}] returns [{result}]");

        self.data
            .parameters
            .update_subscribers(&key, caller_id.clone())
            .await;

        Ok(result.try_to_value()?)
    }
}

/// Handler for retrieving a parameter value from the server.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `key` - Parameter name. If `key` is a namespace, `getParam()` will return a parameter tree (string).
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `parameterValue` - the value of the requested parameter (of type `XMLRPCLegalValue`). If `code` is not 1,
/// `parameterValue` should be ignored. If `key` is a namespace, the return value will be a dictionary, where each
/// key is a parameter in that namespace. Sub-namespaces are also represented as dictionaries.
struct GetParamHandler {
    data: Arc<RosData>,
}
type GetParamResponse = CommonResponse<Value>;
#[async_trait]
impl Handler for GetParamHandler {
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "getParam", skip_all))]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getParam[{params_str}]");
        type Request = (String, String);
        let (caller_id, key) = Request::try_from_params(params)?;
        let key_full = resolve(&caller_id, &key);

        let response = match self.data.parameters.get(&key_full) {
            Ok(Some(value)) => {
                let response = GetParamResponse::new(1, format!("Parameter [{key_full}]"), value);
                log::debug!("getParam[{params_str}] returns [{response}]");
                response
            }
            _ => {
                let response = GetParamResponse::new(
                    -1,
                    format!("Parameter [{key_full}] is not set"),
                    Value::i4(0),
                );
                log::debug!("getParam[{params_str}] returns [{response}]");
                response
            }
        };
        Ok(response.try_to_value()?)
    }
}

struct SearchParamHandler {
    data: Arc<RosData>,
}
type SearchParamResponse = CommonResponse<String>;
#[async_trait]
impl Handler for SearchParamHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "searchParam", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("searchParam[{params_str}]");
        type Request = (String, String);
        let (caller_id, key) = Request::try_from_params(params)?;

        let response = match self.data.parameters.search(&caller_id, &key) {
            Ok(Some(res)) => {
                let formatted_res = format_value(&res);
                let status = format!("Found [{formatted_res}]");
                SearchParamResponse::new(1, status, String::try_from_value(&res)?)
            }
            Ok(None) => {
                let status = format!("Parameter [{params_str}] is not set");
                SearchParamResponse::new(-1, status, "".to_string())
            }
            Err(e) => {
                let status = format!("Error: {e}");
                SearchParamResponse::new(-1, status, "".to_string())
            }
        };

        log::debug!("searchParam[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

/// Handler for subscribing to a parameter value and updates.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `caller_api` - Node API URI of subscriber for paramUpdate callbacks (string)
/// - `key` - Parameter name (string)
///
/// # Returns
///
/// A tuple of integers, a string representing the response, and the parameter value:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `parameterValue` - the parameter value (XML-RPC legal value). If the parameter has not been set yet,
/// the value will be an empty dictionary.
struct SubscribeParamHandler {
    data: Arc<RosData>,
}
type SubscribeParamResponse = CommonResponse<Value>;
#[async_trait]
impl Handler for SubscribeParamHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "subscribeParam", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("subscribeParam[{params_str}]");
        type Request = (String, String, String);
        let (caller_id, caller_api, key) = Request::try_from_params(params)?;
        let key = resolve(&caller_id, &key);

        self.data
            .master_state
            .register_node(&caller_id, &caller_api)
            .await;

        let value = self
            .data
            .parameters
            .subscribe(caller_id.clone(), key.clone(), caller_api)
            .map_err(|e| DxrError::invalid_data(e))?
            .unwrap_or(HashMap::<String, Value>::new().try_to_value()?);

        log::info!("+CACHEDPARAM [{}] by {}", key, caller_id);
        let status = format!("Subscribed to parameter [{}]", &key);
        let response = SubscribeParamResponse::new(1, status, value);
        log::debug!("subscribeParam[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

/// Handler for unsubscribing from a parameter and its updates.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `caller_api` - Node API URI of subscriber (string)
/// - `key` - Parameter name to unsubscribe from (string)
///
/// # Returns
///
/// A tuple of integers and a string representing the response:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `numUnsubscribed` - number of unsubscriptions (either 0 or 1). If this is zero it means that the
/// caller was not subscribed to the parameter. The call still succeeds as the intended final state is reached.
struct UnSubscribeParamHandler {
    data: Arc<RosData>,
}
type UnSubscribeParamResponse = CommonResponse<i32>;
#[async_trait]
impl Handler for UnSubscribeParamHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "unsubscribeParam", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("unsubscribeParam[{params_str}]");
        type Request = (String, String, String);
        let (caller_id, caller_api, key) = Request::try_from_params(params)?;
        let key = resolve(&caller_id, &key);

        let removed = self
            .data
            .parameters
            .unsubscribe(caller_id.clone(), key.clone(), caller_api);

        log::debug!("unsubscribeParam[{params_str}] removed: {removed}");

        let status = if removed {
            format!("Unsubscribe to parameter [{}]", key)
        } else {
            "".to_string()
        };
        let response = UnSubscribeParamResponse::new(1, status, if removed { 1 } else { 0 });
        log::debug!("unsubscribeParam[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

fn resolve(caller_id: &str, key: &str) -> String {
    match key.chars().next() {
        None => "".to_owned(),
        Some('/') => key.to_owned(),
        Some('~') => format!("{}/{}", caller_id, &key[1..]),
        Some(_) => match caller_id.rsplit_once('/') {
            Some((namespace, _node_name)) => format!("{}/{}", namespace, key),
            None => key.to_owned(),
        },
    }
}

/// Handler for checking if a parameter is stored on the server.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
/// - `key` - Parameter name (string)
///
/// # Returns
///
/// A tuple of integers and a boolean representing the response:
///
/// - `code` - Response code (integer)
/// - `statusMessage` - Status message (string)
/// - `hasParam` - Boolean indicating whether the parameter is stored on the server (true) or not (false).
struct HasParamHandler {
    data: Arc<RosData>,
}
type HasParamResponse = CommonResponse<bool>;
#[async_trait]
impl Handler for HasParamHandler {
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "hasParam", skip_all))]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("hasParam[{params_str}]");

        type Request = (String, String);
        let (caller_id, key) = Request::try_from_params(params)?;
        let key = resolve(&caller_id, &key);
        let has = self
            .data
            .parameters
            .contains(&key)
            .map_err(|e| DxrError::invalid_data(e))?;
        let response = HasParamResponse::new(1, key, has);
        log::debug!("hasParam[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

/// Handler for getting a list of all parameter names stored on the server.
///
/// # Parameters
///
/// - `caller_id` - ROS caller ID (string)
///
/// # Returns
///
/// A tuple of integers, a string, and a list of parameter names:
///
/// - `code` - response code (integer)
/// - `statusMessage` - status message (string)
/// - `parameterNameList` - list of all parameter names stored on the server (list of strings)
struct GetParamNamesHandler {
    data: Arc<RosData>,
}
type GetParamNamesResponse = CommonResponse<Vec<String>>;
#[async_trait]
impl Handler for GetParamNamesHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "getParamNames", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("getParamNames[{params_str}]");
        let a = <(String, String)>::try_from_params(params);
        let b = <(String,)>::try_from_params(params);

        if a.is_err() && b.is_err() {
            a?;
        }

        let keys: Vec<String> = self.data.parameters.get_keys();
        let response = GetParamNamesResponse::new(1, "Parameter names".to_string(), keys.clone());
        log::debug!("getParamNames[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

/// Handler for debugging output. This handler logs the incoming request parameters as a debug
/// message and always returns a success response with an empty status message.
///
/// # Parameters
///
/// - `data` - Shared reference to `RosData` struct (Arc<RosData>)
///
/// # Returns
///
/// A `HandlerResult` representing a tuple of integers and a string:
///
/// - `code` - response code (integer)
/// - `ignore` ignored (string, always empty in this case)
/// - `ignore` ignored (string, always empty in this case)
struct DebugOutputHandler {
    #[allow(dead_code)]
    data: Arc<RosData>,
}
#[async_trait]
impl Handler for DebugOutputHandler {
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(name = "system.multicall", skip_all)
    )]
    async fn handle(&self, params: &[Value], _headers: HeaderMap) -> HandlerResult {
        let params_str = param_to_string(params);
        log::debug!("system.multicall[{params_str}]");
        let response = CommonResponse::<String>::new(1, "", "".to_string());
        log::debug!("system.multicall[{params_str}] returns [{response}]");
        Ok(response.try_to_value()?)
    }
}

macro_rules! make_handlers {
    ($self:ident, $($endpoint:expr=>$handlerFn:ident),*) => {{
        let router = RouteBuilder::new()
            $(.add_method($endpoint.as_str(), Box::new($handlerFn {
                data: $self.data.clone(),
            })))*
            .build();
        router
    }};
}

impl Master {
    pub fn new(uri: String) -> Master {
        Master {
            data: Arc::new(RosData {
                master_state: MasterState::default(),
                parameters: ParamTree::default(),
                uri,
            }),
        }
    }

    fn create_router(&self) -> axum::Router {
        let router = make_handlers!(
            self,
            MasterEndpoints::RegisterService => RegisterServiceHandler,
            MasterEndpoints::UnRegisterService => UnRegisterServiceHandler,
            MasterEndpoints::RegisterSubscriber => RegisterSubscriberHandler,
            MasterEndpoints::UnregisterSubscriber => UnRegisterSubscriberHandler,
            MasterEndpoints::RegisterPublisher => RegisterPublisherHandler,
            MasterEndpoints::UnregisterPublisher => UnRegisterPublisherHandler,
            MasterEndpoints::LookupNode => LookupNodeHandler,
            MasterEndpoints::GetPublishedTopics => GetPublishedTopicsHandler,
            MasterEndpoints::GetTopicTypes => GetTopicTypesHandler,
            MasterEndpoints::GetSystemState => GetSystemStateHandler,
            MasterEndpoints::GetUri => GetUriHandler,
            MasterEndpoints::LookupService => LookupServiceHandler,
            MasterEndpoints::DeleteParam => DeleteParamHandler,
            MasterEndpoints::SetParam => SetParamHandler,
            MasterEndpoints::GetParam => GetParamHandler,
            MasterEndpoints::SearchParam => SearchParamHandler,
            MasterEndpoints::SubscribeParam => SubscribeParamHandler,
            MasterEndpoints::UnsubscribeParam => UnSubscribeParamHandler,
            MasterEndpoints::HasParam => HasParamHandler,
            MasterEndpoints::GetParamNames => GetParamNamesHandler,
            MasterEndpoints::SystemMultiCall => DebugOutputHandler,
            MasterEndpoints::GetPid => GetPidHandler,
            MasterEndpoints::Default => DebugOutputHandler
        );
        router
    }

    /// Starts the ROS core server and listens for incoming requests.
    ///
    /// The server will listen on the URI specified during the construction of `RosCoreServer`.
    /// The server router will handle requests to both `/` and `/RPC2`.
    ///
    /// # Returns
    ///
    /// An `anyhow::Result` indicating if the server started successfully or if there was an error.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use ros_core_rs::core::Master;
    /// use url::Url;
    ///
    /// let socket_address = ros_core_rs::url_to_socket_addr(&Url::parse("http://0.0.0.0:11311").unwrap());
    /// let core = Master::new(&socket_address.unwrap());
    /// core.serve();
    /// ```
    pub async fn serve(&self, bind_address: std::net::SocketAddr) -> anyhow::Result<()> {
        // Some ROS implementation use /RPC2 like the python subscribers. Some ROS implementation
        // use / like Foxglove. We serve them all.
        let router: axum::Router = axum::Router::new()
            .nest("/", self.create_router())
            .nest("/RPC2", self.create_router());

        #[cfg(feature = "tracing")]
        let router = router.layer(tower_http::trace::TraceLayer::new_for_http());

        log::info!("roscore-rs is listening on {}", bind_address);
        let server = Server::from_route(router);
        Ok(server.serve(bind_address).await?)
    }

    pub async fn send_requests(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct MasterClient {
    client: Client,
}

macro_rules! implement_client_fn {
    ($name:ident($($v:ident: $t:ty),*)->$response_type:ident) => {
        paste!{
            pub async fn [<$name:snake>](&self, $($v: $t),*) -> anyhow::Result<$response_type>{
                let request = (
                    MasterEndpoints::$name.as_str(),
                    ($($v,)*),
                );
                let response = self.client.call(request.0, request.1).await?;
                let value = $response_type::try_from_value(&response)?;
                Ok(value)
            }
        }
    };
}

macro_rules! make_client{
    ($($name:tt($($v:ident: $t:ty),*)-> $response_type:ident),*) => {
        $(implement_client_fn!($name($($v: $t),*)-> $response_type);)*

    }

}

impl MasterClient {
    /// Constructs a new instance of `MasterClient` with the provided `Url`.
    ///
    /// # Arguments
    ///
    /// * `url` - A `Url` struct representing the ROS master URI to connect to.
    ///
    /// # Example
    ///
    /// ```
    /// use ros_core_rs::core::MasterClient;
    /// use url::Url;
    ///
    /// let uri = Url::parse("http://localhost:11311").unwrap();
    /// let client = MasterClient::new(&uri);
    /// ```
    pub fn new(url: &Url) -> Self {
        let client = ClientBuilder::new(url.clone())
            .user_agent("master-client")
            .build();
        Self { client }
    }

    make_client!(
        RegisterService(caller_id: &str, service: &str, service_api: &str, caller_api: &str) -> RegisterServiceResponse,
        UnRegisterService(caller_id: &str, service: &str, service_api:  &str) -> UnRegisterServiceResponse,
        RegisterSubscriber(caller_id: &str, topic: &str, topic_type: &str, caller_api: &str) -> RegisterSubscriberResponse,
        UnregisterSubscriber(caller_id: &str, topic: &str, caller_api: &str) -> UnRegisterSubscriberResponse,
        RegisterPublisher(caller_id: &str, topic: &str, topic_type: &str, caller_api: &str) -> RegisterPublisherResponse,
        UnregisterPublisher(caller_id: &str, topic: &str, caller_api: &str) -> UnRegisterPublisherResponse,
        LookupNode(caller_id: &str, node_name: &str) -> LookupNodeResponse,
        GetPublishedTopics(caller_id: &str, subgraph: &str) -> GetPublishedTopicsResponse,
        GetTopicTypes(caller_id: &str) -> GetTopicTypesResponse,
        GetSystemState(caller_id: &str) -> GetSystemStateResponse,
        GetUri(caller_id: &str) -> GetUriResponse,
        GetPid(caller_id: &str) -> GetPidResponse,
        LookupService(caller_id: &str, service: &str) -> LookupServiceResponse,
        DeleteParam(caller_id: &str, key: &str) -> DeleteParamResponse,
        // TODO():  correct args
        SetParam(caller_id: &str, key: &str, value: &Value) -> SetParamResponse,
        GetParam(caller_id: &str, key: &str) -> GetParamResponse,
        SearchParam(caller_id: &str, key: &str) -> SearchParamResponse,
        // TODO():  correct args
        SubscribeParam(caller_id: &str, caller_api: &str, keys: &str) -> SubscribeParamResponse,
        UnsubscribeParam(caller_id: &str, caller_api: &str, key: &str) -> UnSubscribeParamResponse,
        HasParam(caller_id: &str, key: &str) -> HasParamResponse,
        GetParamNames(caller_id: &str) -> GetParamNamesResponse
    );
}
