use openfaas_operato_rs::{
    crds::{OpenFaaSFunction, OpenFaasFunctionSpec},
    faas_client::{BasicAuth, FaasCleint},
    request::functions::FunctionDeployment,
};
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

#[tokio::main]
async fn main() {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let basic_auth = BasicAuth::new("user".to_string(), "pass".to_string());
    let faas_client = FaasCleint::new("http://localhost:8081".to_string(), Some(basic_auth));

    let function_deployment = FunctionDeployment {
        open_faas_function_spec: OpenFaasFunctionSpec {
            service: "nodeinfo".to_string(),
            image: "ghcr.io/openfaas/nodeinfo:latest".to_string(),
            namespace: Some("openfaas-fn".to_string()),
            env_process: None,
            env_vars: None,
            constraints: None,
            secrets: None,
            labels: None,
            annotations: None,
            limits: None,
            requests: None,
            read_only_root_filesystem: None,
        },
    };

    // println!("{:?}", serde_json::to_string(&function_deployment).unwrap());

    // faas_client.deploy_function(function_deployment).await;
}
