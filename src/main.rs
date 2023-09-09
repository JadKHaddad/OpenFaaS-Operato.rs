use futures::stream::StreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Namespace, Secret, Service},
};
use kube::{
    api::{ListParams, PostParams},
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
    crds::defs::{
        IntoDeploymentError, IntoServiceError, OpenFaaSFunction, OpenFaasFunctionErrorStatus,
        OpenFaasFunctionOkStatus, OpenFaasFunctionStatus, FINALIZER_NAME,
    },
};
use std::sync::Arc;
use thiserror::Error as ThisError;
use tokio::time::Duration;
use tracing::{trace_span, Instrument, Span};
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var(
            "RUST_LOG",
            "openfaas_operato_rs=trace,tower_http=off,hyper=off",
        );
    }

    tracing_subscriber::fmt()
        //.with_span_events(tracing_subscriber::fmt::format::FmtSpan::ACTIVE)
        //.with_line_number(true)
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
    #[error("Resource has no namespace.")]
    Namespace,

    #[error("Failed to finalize resource: {0}")]
    FinalizeError(#[source] FinalizerError<FinalizeError>),
}

#[derive(ThisError, Debug)]
enum ApplyError {
    #[error("Failed to check resource namespace: {0}")]
    ResourceNamespace(#[source] CheckResourceNamespaceError),
    #[error("Failed to check function namespace: {0}")]
    FunctionNamespace(#[source] CheckFunctionNamespaceError),
    #[error("Deployment error: {0}")]
    Deployment(#[source] DeploymentError),
    #[error("Service error: {0}")]
    Service(#[source] ServiceError),
    #[error("Status error: {0}")]
    Status(#[source] DeployedStatusError),
}

#[derive(ThisError, Debug)]
enum CheckResourceNamespaceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
enum CheckFunctionNamespaceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
#[error("Failed to set satus to {status:?}: {error}")]
struct StatusError {
    #[source]
    error: SetStatusError,
    status: OpenFaasFunctionStatus,
}

#[derive(ThisError, Debug)]
enum SetStatusError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to serialize resource.")]
    Serilization(#[source] serde_json::Error),
}

#[derive(ThisError, Debug)]
enum CheckSecretsError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
enum DeploymentError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to get deployment: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to check secrets: {0}")]
    Secrets(#[source] CheckSecretsError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate deployment: {0}")]
    Generate(#[source] IntoDeploymentError),
    #[error("Failed to apply deployment: {0}")]
    Apply(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
enum ServiceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to get service: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate service: {0}")]
    Generate(#[source] IntoServiceError),
    #[error("Failed to apply service: {0}")]
    Apply(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
enum DeployedStatusError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
enum CleanupError {}

#[derive(ThisError, Debug)]
enum FinalizeError {
    #[error("Failed to apply resource: {0}")]
    Apply(#[source] ApplyError),

    #[error("Failed to cleanup resource: {0}")]
    Cleanup(#[source] CleanupError),
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

    let startup_span = trace_span!("Startup");

    let (functions_namespace, kubernetes_client) = async {
        tracing::info!("Collecting environment variables.");

        let functions_namespace =
            read_from_env_or_default(FUNCTIONS_NAMESPACE_ENV_VAR, FUNCTIONS_DEFAULT_NAMESPACE);

        tracing::info!("Creating kubernetes client.");

        let kubernetes_client = match KubeClient::try_default().await {
            Ok(client) => client,
            Err(error) => {
                tracing::error!(%error, "Failed to create kubernetes client. Exiting.");
                std::process::exit(1);
            }
        };

        let check_namespace_span = trace_span!("CheckNamespace", namespace = %functions_namespace);

        async {
            tracing::info!("Checking if namespace exists.");

            let namespace_api: Api<Namespace> = Api::all(kubernetes_client.clone());
            match namespace_api.get_opt(&functions_namespace).await {
                Ok(namespace_opt) => match namespace_opt {
                    Some(_) => {
                        tracing::info!("Namespace exists.");
                    }
                    None => {
                        tracing::warn!("Namespace does not exist.");
                    }
                },
                Err(error) => {
                    tracing::warn!(%error,"Failed to check if namespace exists.");
                }
            }
        }
        .instrument(check_namespace_span)
        .await;

        (functions_namespace, kubernetes_client)
    }
    .instrument(startup_span)
    .await;

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

    async {
        tracing::info!("Starting.");

        let reconcile_span = trace_span!("Reconcile");

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
            .instrument(reconcile_span)
            .await;

        tracing::info!("Terminated.");
    }
    .instrument(controller_span)
    .await;
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

    let reconcile_resource_span = trace_span!("ReconcileResource", %name, %resource_namespace);

    let api: Api<OpenFaaSFunction> =
        Api::namespaced(context.kubernetes_client.clone(), &resource_namespace);

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

                    apply(
                        api,
                        context,
                        openfaas_function_crd,
                        &name,
                        &resource_namespace,
                    )
                    .instrument(apply_resource_span)
                    .await
                    .map_err(FinalizeError::Apply)
                }
                Event::Cleanup(openfaas_function_crd) => {
                    let cleanup_resource_span = trace_span!("CleanupResource");

                    cleanup(
                        api,
                        context,
                        openfaas_function_crd,
                        &name,
                        &resource_namespace,
                    )
                    .instrument(cleanup_resource_span)
                    .await
                    .map_err(FinalizeError::Cleanup)
                }
            }
        },
    )
    .instrument(reconcile_resource_span)
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

    let check_res_namespace_span = trace_span!("CheckResourceNamespace", %functions_namespace);
    if let Some(action) = check_resource_namespace(
        &api,
        name,
        resource_namespace,
        functions_namespace,
        check_res_namespace_span.clone(),
    )
    .instrument(check_res_namespace_span)
    .await
    .map_err(ApplyError::ResourceNamespace)?
    {
        return Ok(action);
    }

    let check_fun_namespace_span = trace_span!("CheckFunctionNamespace", %functions_namespace);
    if let Some(action) = check_function_namespace(
        &api,
        name,
        &openfaas_function_crd,
        functions_namespace,
        check_fun_namespace_span.clone(),
    )
    .instrument(check_fun_namespace_span)
    .await
    .map_err(ApplyError::FunctionNamespace)?
    {
        return Ok(action);
    }

    let check_deployment_span = trace_span!("CheckDeployment");
    if let Some(action) = check_deployment(
        &api,
        &context,
        &openfaas_function_crd,
        name,
        functions_namespace,
        check_deployment_span.clone(),
    )
    .instrument(check_deployment_span)
    .await
    .map_err(ApplyError::Deployment)?
    {
        return Ok(action);
    }

    let check_service_span = trace_span!("CheckService");
    if let Some(action) = check_service(
        &api,
        &context,
        &openfaas_function_crd,
        name,
        functions_namespace,
        check_service_span.clone(),
    )
    .instrument(check_service_span)
    .await
    .map_err(ApplyError::Service)?
    {
        return Ok(action);
    }

    let set_status_span = trace_span!("SetDeployedStatus");
    if let Some(action) = set_deployed_status(
        &api,
        &context,
        &openfaas_function_crd,
        name,
        functions_namespace,
        set_status_span.clone(),
    )
    .instrument(set_status_span)
    .await
    .map_err(ApplyError::Status)?
    {
        return Ok(action);
    }

    tracing::info!("Awaiting change.");

    Ok(Action::await_change())
}

async fn check_resource_namespace(
    api: &Api<OpenFaaSFunction>,
    name: &str,
    resource_namespace: &str,
    functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, CheckResourceNamespaceError> {
    tracing::info!("Comparing resource's namespace to functions namespace.");

    if resource_namespace != functions_namespace {
        tracing::error!("Resource's namespace does not match functions namespace.");

        let mut openfaas_function_crd_with_status = api
            .get_status(name)
            .await
            .map_err(CheckResourceNamespaceError::Kube)?;

        replace_status(
            api,
            &mut openfaas_function_crd_with_status,
            name,
            OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::InvalidCRDNamespace),
            span.clone(),
        )
        .instrument(span)
        .await
        .map_err(CheckResourceNamespaceError::Status)?;

        tracing::info!("Requeueing resource.");

        return Ok(Some(Action::requeue(Duration::from_secs(10))));
    }

    Ok(None)
}

async fn check_function_namespace(
    api: &Api<OpenFaaSFunction>,
    name: &str,
    openfaas_function_crd: &OpenFaaSFunction,
    functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, CheckFunctionNamespaceError> {
    tracing::info!("Comparing functions's namespace to functions namespace.");

    match openfaas_function_crd.spec.namespace {
        None => {
            tracing::info!(default = %functions_namespace, "Function has no namespace. Assuming default.");
        }

        Some(ref function_namespace) => {
            tracing::info!(%function_namespace, "Function has namespace.");
            tracing::info!(%function_namespace, "Comparing function's namespace to functions namespace.");

            if function_namespace != functions_namespace {
                tracing::error!(%function_namespace, "Function's namespace does not match functions namespace.");

                let mut openfaas_function_crd_with_status = api
                    .get_status(name)
                    .instrument(span.clone())
                    .await
                    .map_err(CheckFunctionNamespaceError::Kube)?;

                replace_status(
                    api,
                    &mut openfaas_function_crd_with_status,
                    name,
                    OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::InvalidFunctionNamespace,
                    ),
                    span.clone(),
                )
                .instrument(span)
                .await
                .map_err(CheckFunctionNamespaceError::Status)?;

                tracing::info!("Requeueing resource.");

                return Ok(Some(Action::requeue(Duration::from_secs(10))));
            }
        }
    }

    Ok(None)
}

async fn check_deployment(
    api: &Api<OpenFaaSFunction>,
    context: &ContextData,
    openfaas_function_crd: &OpenFaaSFunction,
    name: &str,
    functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, DeploymentError> {
    tracing::info!("Checking if deployment exists.");

    let deployment_api: Api<Deployment> =
        Api::namespaced(context.kubernetes_client.clone(), functions_namespace);

    let deployment_opt = deployment_api
        .get_opt(name)
        .instrument(span.clone())
        .await
        .map_err(DeploymentError::Get)?;

    match deployment_opt {
        Some(deployment) => {
            tracing::info!("Deployment exists. Comparing.");

            let crd_oref = openfaas_function_crd
                .controller_owner_ref(&())
                .ok_or(DeploymentError::OwnerReference)?;

            let deployment_orefs = deployment.owner_references();

            if !deployment_orefs.contains(&crd_oref) {
                tracing::error!("Deployment does not have owner reference.");

                let mut openfaas_function_crd_with_status = api
                    .get_status(name)
                    .instrument(span.clone())
                    .await
                    .map_err(DeploymentError::Kube)?;

                replace_status(
                    api,
                    &mut openfaas_function_crd_with_status,
                    name,
                    OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::DeploymentAlreadyExists,
                    ),
                    span.clone(),
                )
                .instrument(span)
                .await
                .map_err(DeploymentError::Status)?;

                tracing::info!("Requeueing resource.");

                return Ok(Some(Action::requeue(Duration::from_secs(10))));
            }

            // TODO: Compare deployment
        }
        None => {
            tracing::info!("Deployment does not exist. Creating.");

            let check_secrets_span = trace_span!("CheckSecrets");
            if let Some(action) = check_secrets(
                api,
                context,
                openfaas_function_crd,
                name,
                functions_namespace,
                check_secrets_span.clone(),
            )
            .instrument(check_secrets_span)
            .await
            .map_err(DeploymentError::Secrets)?
            {
                return Ok(Some(action));
            }

            let deployment =
                Deployment::try_from(openfaas_function_crd).map_err(DeploymentError::Generate)?;
            deployment_api
                .create(&PostParams::default(), &deployment)
                .instrument(span)
                .await
                .map_err(DeploymentError::Apply)?;

            tracing::info!("Deployment created.");
        }
    }

    Ok(None)
}

async fn check_secrets(
    api: &Api<OpenFaaSFunction>,
    context: &ContextData,
    openfaas_function_crd: &OpenFaaSFunction,
    name: &str,
    functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, CheckSecretsError> {
    tracing::info!("Checking if secrets exist.");

    let secrets = openfaas_function_crd.spec.get_secrets_vec();
    if !secrets.is_empty() {
        let secrets_api: Api<Secret> =
            Api::namespaced(context.kubernetes_client.clone(), functions_namespace);

        let existing_secret_names: Vec<String> = secrets_api
            .list(&ListParams::default())
            .await
            .map_err(CheckSecretsError::Kube)?
            .into_iter()
            .map(|secret| secret.metadata.name.unwrap_or_default())
            .collect();

        let not_found_secret_names: Vec<String> = secrets
            .iter()
            .filter(|secret| !existing_secret_names.contains(secret))
            .cloned()
            .collect();

        if !not_found_secret_names.is_empty() {
            let not_found_secret_names_str = not_found_secret_names.join(", ");
            tracing::error!("Secret(s) {} do(es) not exist.", not_found_secret_names_str);

            let mut openfaas_function_crd_with_status = api
                .get_status(name)
                .instrument(span.clone())
                .await
                .map_err(CheckSecretsError::Kube)?;

            replace_status(
                api,
                &mut openfaas_function_crd_with_status,
                name,
                OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::SecretsNotFound),
                span.clone(),
            )
            .instrument(span)
            .await
            .map_err(CheckSecretsError::Status)?;

            tracing::info!("Requeueing resource.");

            return Ok(Some(Action::requeue(Duration::from_secs(10))));
        }
    }

    tracing::info!("Secrets exist.");

    Ok(None)
}

async fn check_service(
    api: &Api<OpenFaaSFunction>,
    context: &ContextData,
    openfaas_function_crd: &OpenFaaSFunction,
    name: &str,
    functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, ServiceError> {
    tracing::info!("Checking if service exists.");

    let service_api: Api<Service> =
        Api::namespaced(context.kubernetes_client.clone(), functions_namespace);

    let service_opt = service_api
        .get_opt(name)
        .instrument(span.clone())
        .await
        .map_err(ServiceError::Get)?;

    match service_opt {
        Some(service) => {
            tracing::info!("Service exists. Comparing.");

            let crd_oref = openfaas_function_crd
                .controller_owner_ref(&())
                .ok_or(ServiceError::OwnerReference)?;

            let service_orefs = service.owner_references();

            if !service_orefs.contains(&crd_oref) {
                tracing::error!("Service does not have owner reference.");

                let mut openfaas_function_crd_with_status = api
                    .get_status(name)
                    .instrument(span.clone())
                    .await
                    .map_err(ServiceError::Kube)?;

                replace_status(
                    api,
                    &mut openfaas_function_crd_with_status,
                    name,
                    OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::ServiceAlreadyExists),
                    span.clone(),
                )
                .instrument(span)
                .await
                .map_err(ServiceError::Status)?;

                tracing::info!("Requeueing resource.");

                return Ok(Some(Action::requeue(Duration::from_secs(10))));
            }

            // TODO: Compare service
        }
        None => {
            tracing::info!("Service does not exist. Creating.");
            let service =
                Service::try_from(openfaas_function_crd).map_err(ServiceError::Generate)?;

            service_api
                .create(&PostParams::default(), &service)
                .instrument(span)
                .await
                .map_err(ServiceError::Apply)?;

            tracing::info!("Service created.");
        }
    }

    Ok(None)
}

async fn set_deployed_status(
    api: &Api<OpenFaaSFunction>,
    _context: &ContextData,
    _openfaas_function_crd: &OpenFaaSFunction,
    name: &str,
    _functions_namespace: &str,
    span: Span,
) -> Result<Option<Action>, DeployedStatusError> {
    tracing::info!("Setting status.");

    let mut openfaas_function_crd_with_status = api
        .get_status(name)
        .instrument(span.clone())
        .await
        .map_err(DeployedStatusError::Kube)?;

    replace_status(
        api,
        &mut openfaas_function_crd_with_status,
        name,
        OpenFaasFunctionStatus::Ok(OpenFaasFunctionOkStatus::Deployed),
        span.clone(),
    )
    .instrument(span)
    .await
    .map_err(DeployedStatusError::Status)?;

    Ok(None)
}

async fn replace_status(
    api: &Api<OpenFaaSFunction>,
    openfaas_function_crd_with_status: &mut OpenFaaSFunction,
    name: &str,
    status: OpenFaasFunctionStatus,
    span: Span,
) -> Result<(), StatusError> {
    match openfaas_function_crd_with_status.status {
        Some(ref func_status) if func_status == &status => {
            tracing::info!("Resource already has {:?} status. Skipping.", status);
        }
        _ => {
            tracing::info!("Setting status to {:?}.", status);

            openfaas_function_crd_with_status.status = Some(status.clone());
            api.replace_status(
                name,
                &PostParams::default(),
                serde_json::to_vec(&openfaas_function_crd_with_status).map_err(|error| {
                    StatusError {
                        error: SetStatusError::Serilization(error),
                        status: status.clone(),
                    }
                })?,
            )
            .instrument(span)
            .await
            .map_err(|error| StatusError {
                error: SetStatusError::Kube(error),
                status: status.clone(),
            })?;
            tracing::info!("Status set to {:?}.", status);
        }
    }
    Ok(())
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
