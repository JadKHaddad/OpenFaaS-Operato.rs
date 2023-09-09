use anyhow::Result as AnyResult;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Namespace, Service},
};
use kube::{Api, Client as KubeClient, Error as KubeError};
use openfaas_operato_rs::{
    consts::*,
    controller::{run_controller, ContextData},
    crds::defs::OpenFaaSFunction,
};
use std::sync::Arc;
use tracing::{trace_span, Instrument, Span};
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "openfaas_operato_rs=trace");
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

fn read_from_env_or_default(env_var: &str, default: &str) -> String {
    tracing::debug!(%env_var, %default, "Reading environment variable.");

    std::env::var(env_var).unwrap_or_else(|_| {
        tracing::warn!(%default, "{env_var} not set, using default.");
        default.to_string()
    })
}

async fn start_up(
    span: Span,
) -> Result<
    (
        Api<OpenFaaSFunction>,
        Api<Deployment>,
        Api<Service>,
        Arc<ContextData>,
    ),
    KubeError,
> {
    tracing::info!("Collecting environment variables.");

    let functions_namespace =
        read_from_env_or_default(FUNCTIONS_NAMESPACE_ENV_VAR, FUNCTIONS_DEFAULT_NAMESPACE);

    tracing::info!("Creating kubernetes client.");

    let kubernetes_client = KubeClient::try_default().instrument(span).await?;

    let check_namespace_span = trace_span!("CheckNamespace", namespace = %functions_namespace);
    check_namespace(kubernetes_client.clone(), &functions_namespace)
        .instrument(check_namespace_span)
        .await;

    let crd_api: Api<OpenFaaSFunction> = Api::all(kubernetes_client.clone());

    let deployment_api: Api<Deployment> =
        Api::namespaced(kubernetes_client.clone(), &functions_namespace);
    let service_api: Api<Service> =
        Api::namespaced(kubernetes_client.clone(), &functions_namespace);

    let context = Arc::new(ContextData::new(kubernetes_client, functions_namespace));

    Ok((crd_api, deployment_api, service_api, context))
}

async fn check_namespace(kubernetes_client: KubeClient, functions_namespace: &str) {
    tracing::info!("Checking if namespace exists.");

    let namespace_api: Api<Namespace> = Api::all(kubernetes_client);
    match namespace_api.get_opt(functions_namespace).await {
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

#[tokio::main]
async fn main() -> AnyResult<()> {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let startup_span = trace_span!("Startup");
    let (crd_api, deployment_api, service_api, context) = start_up(startup_span.clone())
        .instrument(startup_span)
        .await
        .map_err(|error| {
            tracing::error!(%error, "Failed to create kubernetes client. Exiting.");
            error
        })?;

    let controller_span = trace_span!("Controller");
    run_controller(crd_api, deployment_api, service_api, context)
        .instrument(controller_span)
        .await;

    Ok(())
}
