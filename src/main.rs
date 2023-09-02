use futures::stream::StreamExt;
use kube::{
    runtime::Controller,
    runtime::{controller::Action, watcher::Config},
    Api, Client as KubeClient, Error as KubeError,
};
use openfaas_operato_rs::{
    crds::OpenFaaSFunction,
    faas_client::{BasicAuth, FaasCleint},
};
use std::sync::Arc;
use thiserror::Error as ThisError;
use tracing_subscriber::EnvFilter;

const FAAS_GATEWAY_DEFAULT_URL: &str = "http://gateway.openfaas.svc.cluster.local:8080";

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

#[tokio::main]
async fn main() {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let faas_gateway_url = std::env::var("FAAS_GATEWAY_URL").unwrap_or_else(|_| {
        tracing::warn!(
            faas_gateway_url = FAAS_GATEWAY_DEFAULT_URL,
            "FAAS_GATEWAY_URL not set, using default"
        );
        FAAS_GATEWAY_DEFAULT_URL.to_string()
    });

    let faas_gateway_url: String = match url::Url::parse(&faas_gateway_url) {
        Ok(mut parsed_url) => {
            if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                tracing::warn!("FAAS_GATEWAY_URL must be a http or https url. Assuming http");
                parsed_url.set_scheme("http").unwrap_or_else(|_| {
                    tracing::error!("Failed to set scheme to http. Exiting");
                    std::process::exit(1);
                });
            }

            let host = parsed_url.host_str().unwrap_or_else(|| {
                tracing::error!("Failed to get host from FAAS_GATEWAY_URL. Exiting");
                std::process::exit(1);
            });

            tracing::info!(faas_gateway_host = host, "Performing dns lookup");
            match tokio::net::lookup_host(host).await {
                Ok(mut addresses) => {
                    if addresses.next().is_none() {
                        tracing::warn!("No ip addresses found for host");
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "Dns lookup failed");
                }
            };

            parsed_url.into()
        }
        Err(error) => {
            tracing::error!(%error, "Failed to parse FAAS_GATEWAY_URL. Exiting");
            std::process::exit(1);
        }
    };

    let gateway_username = std::env::var("FAAS_GATEWAY_USERNAME").ok();
    let gateway_password = std::env::var("FAAS_GATEWAY_PASSWORD").ok();
    let basic_auth = match (gateway_username, gateway_password) {
        (Some(username), Some(password)) => Some(BasicAuth::new(username, password)),
        _ => {
            tracing::warn!("FAAS_GATEWAY_USERNAME or FAAS_GATEWAY_PASSWORD not set");
            None
        }
    };

    let faas_client = FaasCleint::new(faas_gateway_url, basic_auth);
    let kubernetes_client = match KubeClient::try_default().await {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "Failed to create kubernetes client. Exiting");
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
                    tracing::info!("Reconciliation successful");
                }
                Err(error) => {
                    tracing::error!(%error, "Reconciliation failed");
                }
            }
        })
        .await;

    // let function_deployment = FunctionDeployment {
    //     open_faas_function_spec: OpenFaasFunctionSpec {
    //         service: "nodeinfo".to_string(),
    //         image: "ghcr.io/openfaas/nodeinfo:latest".to_string(),
    //         namespace: Some("openfaas-fn".to_string()),
    //         env_process: None,
    //         env_vars: None,
    //         constraints: None,
    //         secrets: None,
    //         labels: None,
    //         annotations: None,
    //         limits: None,
    //         requests: None,
    //         read_only_root_filesystem: None,
    //     },
    // };

    // println!("{:?}", serde_json::to_string(&function_deployment).unwrap());

    // faas_client.deploy_function(function_deployment).await;
}

async fn reconcile(
    openfaas_function: Arc<OpenFaaSFunction>,
    context: Arc<ContextData>,
) -> Result<Action, ControllerError> {
    todo!()
}

fn on_error(
    openfaas_function: Arc<OpenFaaSFunction>,
    error: &ControllerError,
    _context: Arc<ContextData>,
) -> Action {
    todo!()
}
