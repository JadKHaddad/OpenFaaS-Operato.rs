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
    Api, Client as KubeClient, Error as KubeError, ResourceExt,
};
use openfaas_operato_rs::{
    consts::*,
    crds::{IntoDeploymentError, OpenFaaSFunction, OpenFaasFunctionStatus, FINALIZER_NAME},
};
use std::sync::Arc;
use thiserror::Error as ThisError;
use tokio::time::Duration;
use tracing::trace_span;
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
    #[error("Failed to get deployment: {0}")]
    DeploymentGet(#[source] KubeError),
    #[error("Failed to generate deployment: {0}")]
    DeploymentGenerate(
        #[from]
        #[source]
        IntoDeploymentError,
    ),
    #[error("Failed to apply deployment: {0}")]
    DeploymentApply(#[source] KubeError),
}

#[derive(ThisError, Debug)]
enum CleanupError {}

#[derive(ThisError, Debug)]
enum FinalizeError {
    #[error("Failed to apply resource: {0}")]
    Apply(
        #[from]
        #[source]
        ApplyError,
    ),

    #[error("Failed to cleanup resource: {0}")]
    Cleanup(
        #[from]
        #[source]
        CleanupError,
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
    match namespace_api.get_opt(&functions_namespace).await {
        Ok(namespace_opt) => match namespace_opt {
            Some(_) => {
                tracing::info!(namespace = %functions_namespace, "Namespace exists.");
            }
            None => {
                tracing::warn!(namespace = %functions_namespace, "Namespace does not exist.");
            }
        },
        Err(error) => {
            tracing::warn!(%error, namespace = %functions_namespace, "Failed to check if namespace exists.");
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

    let controller_span = trace_span!("Controller");
    let _controller_span_guard = controller_span.enter();

    tracing::info!("Starting.");

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

    tracing::info!("Terminated.");
}

async fn reconcile(
    openfaas_function_crd: Arc<OpenFaaSFunction>,
    context: Arc<ContextData>,
) -> Result<Action, ReconcileError> {
    let reconcile_span = trace_span!("Reconcile");
    let _reconcile_span_guard = reconcile_span.enter();

    let name = openfaas_function_crd.name_any();

    let resource_namespace: String = match openfaas_function_crd.namespace() {
        None => {
            tracing::error!(%name, "Resource has no namespace. Not even default. Aborting.");
            return Err(ReconcileError::Namespace);
        }

        Some(namespace) => namespace,
    };

    let reconcile_resource_span = trace_span!("ReconcileResource", %name, %resource_namespace);
    let _reconcile_resource_span_guard = reconcile_resource_span.enter();

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

                match event {
                    Event::Apply(openfaas_function_crd) => {
                        let apply_resource_span = trace_span!("ApplyResource");
                        let _apply_resource_span_guard = apply_resource_span.enter();

                        apply(
                            api,
                            context,
                            openfaas_function_crd,
                            &name,
                            &resource_namespace,
                        )
                        .await
                        .map_err(FinalizeError::Apply)
                    }
                    Event::Cleanup(openfaas_function_crd) => {
                        let cleanup_resource_span = trace_span!("CleanupResource");
                        let _cleanup_resource_span_guard = cleanup_resource_span.enter();

                        cleanup(
                            api,
                            context,
                            openfaas_function_crd,
                            &name,
                            &resource_namespace,
                        )
                        .await
                        .map_err(FinalizeError::Cleanup)
                    }
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
    tracing::info!("Applying resource.");
    let functions_namespace = &context.functions_namespace;

    {
        let check_res_namespace_span = trace_span!("CheckResourceNamespace", %functions_namespace);
        let _check_res_namespace_span_guard = check_res_namespace_span.enter();

        tracing::info!("Comparing resource's namespace to functions namespace.");
        if resource_namespace != functions_namespace {
            tracing::error!("Resource's namespace does not match functions namespace.");

            let mut openfaas_function_crd_inner = api.get_status(name).await?;
            match openfaas_function_crd_inner.status {
                Some(OpenFaasFunctionStatus::InvalidCRDNamespace) => {
                    tracing::info!("Resource already has invalid crd namespace status. Skipping.");
                }
                _ => {
                    tracing::info!("Setting status to invalid crd namespace.");

                    openfaas_function_crd_inner.status =
                        Some(OpenFaasFunctionStatus::InvalidCRDNamespace);
                    api.replace_status(
                        name,
                        &PostParams::default(),
                        serde_json::to_vec(&openfaas_function_crd_inner)?,
                    )
                    .await?;

                    tracing::info!("Status set to invalid crd namespace.");
                }
            }

            tracing::info!("Requeueing resource.");

            return Ok(Action::requeue(Duration::from_secs(10)));
        }
    }
    {
        let check_fun_namespace_span = trace_span!("CheckFunctionNamespace", %functions_namespace);
        let _check_fun_namespace_span_guard = check_fun_namespace_span.enter();

        match openfaas_function_crd.spec.namespace {
            None => {
                tracing::info!(default = %functions_namespace, "Function has no namespace. Assuming default.");
            }

            Some(ref function_namespace) => {
                tracing::info!(%function_namespace, "Function has namespace.");
                tracing::info!(%function_namespace, "Comparing function's namespace to functions namespace.");

                if function_namespace != functions_namespace {
                    tracing::error!(%function_namespace, "Function's namespace does not match functions namespace.");

                    let mut openfaas_function_crd_inner = api.get_status(name).await?;
                    match openfaas_function_crd_inner.status {
                        Some(OpenFaasFunctionStatus::InvalidFunctionNamespace) => {
                            tracing::info!(%function_namespace, "Resource already has invalid function namespace status. Skipping.");
                        }
                        _ => {
                            tracing::info!(%function_namespace, "Setting status to invalid function namespace.");

                            openfaas_function_crd_inner.status =
                                Some(OpenFaasFunctionStatus::InvalidFunctionNamespace);
                            api.replace_status(
                                name,
                                &PostParams::default(),
                                serde_json::to_vec(&openfaas_function_crd_inner)?,
                            )
                            .await?;

                            tracing::info!(%function_namespace, "Status set to invalid function namespace.");
                        }
                    }

                    tracing::info!("Requeueing resource.");

                    return Ok(Action::requeue(Duration::from_secs(10)));
                }
            }
        }
    }
    {
        let check_deployment_span = trace_span!("CheckDeployment");
        let _check_deployment_span_guard = check_deployment_span.enter();

        tracing::info!("Checking if deployment exists.");
        let deployment_api: Api<Deployment> =
            Api::namespaced(context.kubernetes_client.clone(), functions_namespace);

        let deployment_opt = deployment_api
            .get_opt(name)
            .await
            .map_err(ApplyError::DeploymentGet)?;

        match deployment_opt {
            Some(deployment) => {
                tracing::info!("Deployment exists. Comparing.");
                // TODO: Check if the controller has deployed the deployment. if not set status to already exists and return with error
            }
            None => {
                tracing::info!("Deployment does not exist. Creating.");

                let deployment = Deployment::try_from(&*openfaas_function_crd)?;
                deployment_api
                    .create(&PostParams::default(), &deployment)
                    .await
                    .map_err(ApplyError::DeploymentApply)?;

                tracing::info!("Deployment created.");
            }
        }
    }
    {
        let check_service_span = trace_span!("CheckService");
        let _check_service_span_guard = check_service_span.enter();

        tracing::info!("Checking if service exists.");

        let service_api: Api<Service> =
            Api::namespaced(context.kubernetes_client.clone(), functions_namespace);

        let service_opt = service_api.get_opt(name).await?;

        match service_opt {
            Some(service) => {
                tracing::info!("Service exists. Comparing.");
            }
            None => {
                tracing::info!("Service does not exist. Creating.");
            }
        }
    }

    // after deploying the deployment and service, set status to deployed
    // if deployment and service are ready, set status to ready

    tracing::info!("Requeueing resource.");
    Ok(Action::requeue(Duration::from_secs(15)))
}

async fn cleanup(
    _api: Api<OpenFaaSFunction>,
    _context: Arc<ContextData>,
    _openfaas_function_crd: Arc<OpenFaaSFunction>,
    _name: &str,
    _resource_namespace: &str,
) -> Result<Action, CleanupError> {
    tracing::info!("Cleaning up resource.");

    tracing::info!("Awaiting change.");
    Ok(Action::await_change())
}

fn on_error(
    _openfaas_function: Arc<OpenFaaSFunction>,
    _error: &ReconcileError,
    _context: Arc<ContextData>,
) -> Action {
    Action::requeue(Duration::from_secs(10))
}
