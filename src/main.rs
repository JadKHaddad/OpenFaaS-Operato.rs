use futures::stream::StreamExt;
use k8s_openapi::api::{apps::v1::Deployment, core::v1::Namespace};
use kube::{
    api::{Patch, PatchParams, PostParams},
    core::object::HasStatus,
    error::ErrorResponse,
    runtime::{
        controller::Action,
        watcher::{self, Config},
    },
    runtime::{finalizer::Event, Controller},
    Api, Client as KubeClient, Error as KubeError, Resource, ResourceExt,
};
use openfaas_operato_rs::{consts::*, crds::OpenFaaSFunction};
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
    kubernetes_client: KubeClient,
}

#[derive(ThisError, Debug)]
enum ReconcileError {
    #[error("Kubernetes error: {0}")]
    Kube(
        #[from]
        #[source]
        KubeError,
    ),
    #[error("Resource has no namespace.")]
    Namespace,
    #[error("Failed to serialize resource.")]
    Serilization(
        #[from]
        #[source]
        serde_json::Error,
    ),
}

fn read_from_env_or_default(env_var: &str, default: &str) -> String {
    std::env::var(env_var).unwrap_or_else(|_| {
        tracing::warn!(%default, "{env_var} not set, using default.");
        default.to_string()
    })
}

#[tokio::main]
async fn main() {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let functions_namespace =
        read_from_env_or_default(FUNCTIONS_NAMESPACE_ENV_VAR, FUNCTIONS_DEFAULT_NAMESPACE);

    let kubernetes_client = match KubeClient::try_default().await {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "Failed to create kubernetes client. Exiting.");
            std::process::exit(1);
        }
    };

    tracing::info!(namespace = %functions_namespace, "Checking if namespace exists.");
    let namespace_api: Api<Namespace> = Api::all(kubernetes_client.clone());
    match namespace_api.get(&functions_namespace).await {
        Ok(_) => {
            tracing::info!(namespace = %functions_namespace, "Namespace exists.");
        }
        Err(error) => {
            if let KubeError::Api(ErrorResponse { code: 404, .. }) = error {
                tracing::warn!(%error, namespace = %functions_namespace, "Namespace does not exist.");
            } else {
                tracing::warn!(%error, namespace = %functions_namespace, "Failed to check if namespace exists.");
            }
        }
    }

    let crd_api: Api<OpenFaaSFunction> = Api::all(kubernetes_client.clone());
    let deployment_api: Api<Deployment> =
        Api::namespaced(kubernetes_client.clone(), &functions_namespace);

    let context = Arc::new(ContextData { kubernetes_client });

    tracing::info!("Starting controller.");
    Controller::new(crd_api, Config::default())
        .owns(deployment_api, Config::default())
        .shutdown_on_signal()
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
    tracing::info!("Controller terminated.");
}

async fn reconcile(
    openfaas_function: Arc<OpenFaaSFunction>,
    context: Arc<ContextData>,
) -> Result<Action, ReconcileError> {
    let name = openfaas_function.name_any();
    let kubernetes_client = &context.kubernetes_client;

    let namespace: String = match openfaas_function.namespace() {
        None => {
            tracing::error!(%name, "Resource has no namespace. Aborting.");
            return Err(ReconcileError::Namespace);
        }

        Some(namespace) => namespace,
    };

    tracing::info!(%name, %namespace, "Reconciling resource.");
    tracing::debug!("Resource data.\n\n{:#?}\n", openfaas_function);

    let api: Api<OpenFaaSFunction> = Api::namespaced(kubernetes_client.clone(), &namespace);

    match determine_reconcile_action(openfaas_function.as_ref()) {
        ReconcileAction::Apply => {
            tracing::info!(%name, %namespace, "Applying resource.");

            Ok(Action::requeue(Duration::from_secs(10)))
        }
        ReconcileAction::Delete => {
            tracing::info!(%name, %namespace, "Deleting resource.");

            Ok(Action::requeue(Duration::from_secs(10)))
        }
    }
}

fn on_error(
    openfaas_function: Arc<OpenFaaSFunction>,
    error: &ReconcileError,
    _context: Arc<ContextData>,
) -> Action {
    Action::requeue(Duration::from_secs(10))
}

fn determine_reconcile_action(openfaas_function: &OpenFaaSFunction) -> ReconcileAction {
    if openfaas_function.meta().deletion_timestamp.is_some() {
        return ReconcileAction::Delete;
    }

    ReconcileAction::Apply
}

enum ReconcileAction {
    Apply,
    /// Since we are not setting a finalizer, we may not be notified of deletion.
    Delete,
}
