use futures::stream::StreamExt;
use kube::{
    api::{Patch, PatchParams},
    runtime::Controller,
    runtime::{controller::Action, watcher::Config},
    Api, Client as KubeClient, Error as KubeError, Resource, ResourceExt,
};
use openfaas_operato_rs::{
    consts::*,
    crds::{OpenFaaSFunction, FINALIZER},
    faas_client::{BasicAuth, FaasCleint},
};
use serde_json::{json, Value};
use std::sync::Arc;
use thiserror::Error as ThisError;
use tokio::time::Duration;
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var(
            "RUST_LOG",
            "openfaas_operato_rs=trace,tower_http=off,hyper=off",
        );
    }

    tracing_subscriber::fmt()
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::UtcTime::rfc_3339())
        .with_level(true)
        .with_ansi(true)
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

struct ContextData {
    faas_client: FaasCleint,
    kubernetes_client: KubeClient,
}

#[derive(ThisError, Debug)]
enum ControllerError {
    #[error("Kubernetes error: {0}")]
    KubeError(
        #[from]
        #[source]
        KubeError,
    ),
    #[error("Invalid CRD")]
    InputError,
}

fn read_from_env_or_default(env_var: &str, default: &str) -> String {
    std::env::var(env_var).unwrap_or_else(|_| {
        tracing::warn!(%default, "{env_var} not set, using default.");
        default.to_string()
    })
}

async fn parse_url_and_perform_dns_lookup(url: &str, url_name: &str) -> url::Url {
    match url::Url::parse(url) {
        Ok(parsed_url) => {
            if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                tracing::error!(url = %parsed_url, "{url_name} must be an http or https url. Exiting.");
                std::process::exit(1);
            }

            let host = parsed_url.host_str().unwrap_or_else(|| {
                tracing::error!(url = %parsed_url, "Failed to get host from url. Exiting.");
                std::process::exit(1);
            });

            tracing::info!(%host, "Performing dns lookup.");
            match tokio::net::lookup_host(host).await {
                Ok(mut addresses) => {
                    if addresses.next().is_none() {
                        tracing::warn!(%host,"No ip addresses found for host.");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, %host, "Dns lookup failed.");
                }
            };

            parsed_url
        }
        Err(error) => {
            tracing::error!(%error, "Failed to parse {url_name}. Exiting");
            std::process::exit(1);
        }
    }
}

fn read_basic_auth_from_env() -> Option<BasicAuth> {
    let gateway_username = std::env::var(FAAS_GATEWAY_USERNAME_ENV_VAR).ok();
    let gateway_password = std::env::var(FAAS_GATEWAY_PASSWORD_ENV_VAR).ok();
    match (gateway_username, gateway_password) {
        (Some(username), Some(password)) => Some(BasicAuth::new(username, password)),
        _ => {
            tracing::warn!(
                "{FAAS_GATEWAY_USERNAME_ENV_VAR} or {FAAS_GATEWAY_PASSWORD_ENV_VAR} not set.",
            );
            None
        }
    }
}

#[tokio::main]
async fn main() {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let faas_gateway_url_str =
        read_from_env_or_default(FAAS_GATEWAY_URL_ENV_VAR, FAAS_GATEWAY_DEFAULT_URL);

    let faas_gateway_url =
        parse_url_and_perform_dns_lookup(&faas_gateway_url_str, FAAS_GATEWAY_URL_ENV_VAR).await;

    let basic_auth = read_basic_auth_from_env();

    let functions_namespace = read_from_env_or_default(
        FAAS_FUNCTIONS_NAMESPACE_ENV_VAR,
        FAAS_FUNCTIONS_DEFAULT_NAMESPACE,
    );

    let faas_client = FaasCleint::new(faas_gateway_url, basic_auth).unwrap_or_else(|error| {
        tracing::error!(%error, "Failed to create faas client. Exiting.");
        std::process::exit(1);
    });

    let kubernetes_client = match KubeClient::try_default().await {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "Failed to create kubernetes client. Exiting.");
            std::process::exit(1);
        }
    };
    let crd_api: Api<OpenFaaSFunction> = Api::all(kubernetes_client.clone());
    let context = Arc::new(ContextData {
        faas_client,
        kubernetes_client,
    });

    Controller::new(crd_api.clone(), Config::default())
        .run(reconcile, on_error, context)
        .for_each(|reconciliation_result| async move {
            match reconciliation_result {
                Ok(_) => {
                    tracing::info!("Reconciliation successful.");
                }
                Err(error) => {
                    tracing::error!(%error, "Reconciliation failed.");
                }
            }
        })
        .await;
}

async fn reconcile(
    openfaas_function: Arc<OpenFaaSFunction>,
    context: Arc<ContextData>,
) -> Result<Action, ControllerError> {
    let name = openfaas_function.name_any();

    let kubernetes_client = context.kubernetes_client.clone();
    let namespace: String = match openfaas_function.namespace() {
        None => {
            tracing::error!(%name, "Resource has no namespace. Aborting.");
            return Err(ControllerError::InputError);
        }
        Some(namespace) => namespace,
    };

    tracing::info!(%name, %namespace, "Reconciling resource.");

    // if the resource is being deleted, remove finalizers and clean up
    if openfaas_function
        .as_ref()
        .meta()
        .deletion_timestamp
        .is_some()
    {
        tracing::info!(%name, %namespace, "Resource is being deleted.");
        remove_finalizers(kubernetes_client.clone(), &name, &namespace).await?;
        tracing::info!(%name, %namespace, "Finalizers removed.");
        return Ok(Action::await_change());
    }

    // if there is no finalizer, add one
    if openfaas_function
        .as_ref()
        .meta()
        .finalizers
        .as_ref()
        .map_or(true, |finalizers| finalizers.is_empty())
    {
        tracing::info!(%name, %namespace, "No finalizer found.");
        add_finalizer(kubernetes_client.clone(), &name, &namespace).await?;
        tracing::info!(%name, %namespace, "Finalizer added.");
        return Ok(Action::await_change());
    }

    tracing::info!(%name, %namespace, "No action required.");
    Ok(Action::await_change())
}

fn on_error(
    openfaas_function: Arc<OpenFaaSFunction>,
    error: &ControllerError,
    _context: Arc<ContextData>,
) -> Action {
    Action::requeue(Duration::from_secs(10))
}

async fn add_finalizer(
    client: KubeClient,
    name: &str,
    namespace: &str,
) -> Result<OpenFaaSFunction, KubeError> {
    let api: Api<OpenFaaSFunction> = Api::namespaced(client, namespace);
    // check for resource existence
    let resource = api.get(name).await?;
    // check if finalizer already exists
    if resource
        .metadata
        .finalizers
        .as_ref()
        .map(|finalizers| finalizers.contains(&FINALIZER.to_string()))
        .unwrap_or(false)
    {
        return Ok(resource);
    }

    let finalizers: Value = json!({
        "metadata": {
            "finalizers": [FINALIZER]
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizers);
    api.patch(name, &PatchParams::default(), &patch).await
}

pub async fn remove_finalizers(
    client: KubeClient,
    name: &str,
    namespace: &str,
) -> Result<OpenFaaSFunction, KubeError> {
    let api: Api<OpenFaaSFunction> = Api::namespaced(client, namespace);
    // check for resource existence
    let _resource = api.get(name).await?;
    let finalizers: Value = json!({
        "metadata": {
            "finalizers": null
        }
    });

    let patch: Patch<&Value> = Patch::Merge(&finalizers);
    api.patch(name, &PatchParams::default(), &patch).await
}
