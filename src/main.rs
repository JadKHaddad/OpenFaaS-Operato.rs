use openfaas_operato_rs::types::FunctionDeployment;
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

    let dep = FunctionDeployment {
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
    };

    let client = reqwest::Client::new();
    let resp = client
        .post("http://localhost:8080/system/functions")
        .basic_auth("user", Some("pass"))
        .header("Content-Type", "application/json")
        // .header("User-Agent", "faas-cli/0.16.13")
        .body(serde_json::to_string(&dep).unwrap())
        .send()
        .await
        .unwrap();

    println!("{:?}", resp);
}
