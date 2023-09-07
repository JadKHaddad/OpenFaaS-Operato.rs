use futures::stream::StreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Namespace, Service},
};
use kube::{
    api::PostParams,
    error::ErrorResponse,
    runtime::{
        controller::Action,
        finalizer::{Error as FinalizerError, Event},
        watcher::Config,
    },
    runtime::{finalizer, Controller},
    Api, Client as KubeClient, Error as KubeError, Resource, ResourceExt,
};
use openfaas_operato_rs::{
    consts::*,
    crds::{OpenFaaSFunction, OpenFaasFunctionStatus, FINALIZER_NAME},
};
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
    functions_namespace: String,
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

    #[error(transparent)]
    FinalizeError(#[from] FinalizerError<FinalizeError>),
}

#[derive(ThisError, Debug)]
enum ApplyError {
    #[error("Kubernetes error: {0}")]
    Kube(
        #[from]
        #[source]
        KubeError,
    ),
    #[error("Failed to serialize resource.")]
    Serilization(
        #[from]
        #[source]
        serde_json::Error,
    ),
}

#[derive(ThisError, Debug)]
enum DeleteError {}

#[derive(ThisError, Debug)]
enum FinalizeError {
    #[error("Failed to apply resource: {0}")]
    Apply(
        #[from]
        #[source]
        ApplyError,
    ),

    #[error("Failed to delete resource: {0}")]
    Delete(
        #[from]
        #[source]
        DeleteError,
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
    let service_api: Api<Service> =
        Api::namespaced(kubernetes_client.clone(), &functions_namespace);

    let context = Arc::new(ContextData {
        kubernetes_client,
        functions_namespace,
    });

    tracing::info!("Starting controller.");

    Controller::new(crd_api, Config::default())
        .owns(deployment_api, Config::default())
        .owns(service_api, Config::default())
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
    openfaas_function_crd: Arc<OpenFaaSFunction>,
    context: Arc<ContextData>,
) -> Result<Action, ReconcileError> {
    let name = openfaas_function_crd.name_any();

    let resource_namespace: String = match openfaas_function_crd.namespace() {
        None => {
            tracing::error!(%name, "Resource has no namespace. Not even default. Aborting.");
            return Err(ReconcileError::Namespace);
        }

        Some(namespace) => namespace,
    };

    tracing::info!(%name, %resource_namespace, "Reconciling resource.");

    let api: Api<OpenFaaSFunction> =
        Api::namespaced(context.kubernetes_client.clone(), &resource_namespace);

    async move {
        let resource_namespace = resource_namespace.clone();
        finalizer(
            &api,
            FINALIZER_NAME,
            openfaas_function_crd,
            |event| async move {
                let api: Api<OpenFaaSFunction> =
                    Api::namespaced(context.kubernetes_client.clone(), &resource_namespace);

                // TODO: create spans for each event Apply and Cleanup
                match event {
                    Event::Apply(openfaas_function_crd) => apply(
                        api,
                        context,
                        openfaas_function_crd,
                        &name,
                        &resource_namespace,
                    )
                    .await
                    .map_err(FinalizeError::Apply),
                    Event::Cleanup(_) => cleanup(&name, &resource_namespace)
                        .await
                        .map_err(FinalizeError::Delete),
                }
            },
        )
        .await
    }
    .await
    .map_err(ReconcileError::FinalizeError)
}

async fn apply(
    api: Api<OpenFaaSFunction>,
    context: Arc<ContextData>,
    openfaas_function_crd: Arc<OpenFaaSFunction>,
    name: &str,
    resource_namespace: &str,
) -> Result<Action, ApplyError> {
    tracing::info!(%name, %resource_namespace, "Applying resource.");

    let functions_namespace = &context.functions_namespace;

    tracing::info!(%name, %resource_namespace, %functions_namespace, "Comparing resource's namespace to functions namespace.");
    if resource_namespace != functions_namespace {
        tracing::error!(%name, %resource_namespace, %functions_namespace, "Resource's namespace does not match functions namespace.");

        let mut openfaas_function_crd_inner = api.get_status(name).await?;
        match openfaas_function_crd_inner.status {
            Some(OpenFaasFunctionStatus::InvalidCRDNamespace) => {
                tracing::info!(%name, %resource_namespace, "Resource already has invalid crd namespace status. Skipping.");
            }
            _ => {
                tracing::info!(%name, %resource_namespace, "Setting status to invalid crd namespace.");

                openfaas_function_crd_inner.status =
                    Some(OpenFaasFunctionStatus::InvalidCRDNamespace);
                api.replace_status(
                    name,
                    &PostParams::default(),
                    serde_json::to_vec(&openfaas_function_crd_inner)?,
                )
                .await?;

                tracing::info!(%name, %resource_namespace, "Status set to invalid crd namespace.");
            }
        }

        tracing::info!(%name, %resource_namespace, "Requeueing resource.");

        return Ok(Action::requeue(Duration::from_secs(10)));
    }

    match openfaas_function_crd.spec.namespace {
        None => {
            tracing::info!(%name, %resource_namespace, default = %functions_namespace, "Function has no namespace. Assuming default.");
        }

        Some(ref function_namespace) => {
            tracing::info!(%name, %resource_namespace, %function_namespace, "Function has namespace.");
            tracing::info!(%name, %resource_namespace, %function_namespace, %functions_namespace, "Comparing function's namespace to functions namespace.");

            if function_namespace != functions_namespace {
                tracing::error!(%name, %resource_namespace, %function_namespace, %functions_namespace, "Function's namespace does not match functions namespace.");

                let mut openfaas_function_crd_inner = api.get_status(name).await?;
                match openfaas_function_crd_inner.status {
                    Some(OpenFaasFunctionStatus::InvalidFunctionNamespace) => {
                        tracing::info!(%name, %resource_namespace, %function_namespace, %functions_namespace, "Resource already has invalid function namespace status. Skipping.");
                    }
                    _ => {
                        tracing::info!(%name, %resource_namespace, %function_namespace, %functions_namespace, "Setting status to invalid function namespace.");

                        openfaas_function_crd_inner.status =
                            Some(OpenFaasFunctionStatus::InvalidFunctionNamespace);
                        api.replace_status(
                            name,
                            &PostParams::default(),
                            serde_json::to_vec(&openfaas_function_crd_inner)?,
                        )
                        .await?;

                        tracing::info!(%name, %resource_namespace, %function_namespace, %functions_namespace, "Status set to invalid function namespace.");
                    }
                }

                tracing::info!(%name, %resource_namespace, "Requeueing resource.");

                return Ok(Action::requeue(Duration::from_secs(10)));
            }
        }
    }

    // compare resource to deployment
    // compare resource to service
    // if everything matches, set status to deployed
    // if not, update deployment and service and requeue

    tracing::info!(%name, %resource_namespace, "Requeueing resource.");
    Ok(Action::requeue(Duration::from_secs(10)))
}

async fn cleanup(name: &str, resource_namespace: &str) -> Result<Action, DeleteError> {
    tracing::info!(%name, %resource_namespace, "Deleting resource.");

    tracing::info!(%name, %resource_namespace, "Awaiting change.");
    Ok(Action::await_change())
}

fn on_error(
    _openfaas_function: Arc<OpenFaaSFunction>,
    _error: &ReconcileError,
    _context: Arc<ContextData>,
) -> Action {
    Action::requeue(Duration::from_secs(10))
}
