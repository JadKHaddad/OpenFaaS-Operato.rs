use anyhow::Result as AnyResult;
use kube::Client as KubeClient;
use openfaas_operato_rs::{
    consts::*, controller::operator::Operator, crds::defs::OpenFaaSFunction,
};
use tracing::{trace_span, Instrument};
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

fn start_up() -> String {
    tracing::info!("Collecting environment variables.");

    read_from_env_or_default(FUNCTIONS_NAMESPACE_ENV_VAR, FUNCTIONS_DEFAULT_NAMESPACE)
}

async fn create_and_run_operator(client: KubeClient, functions_namespace: String) {
    let span = trace_span!("Create", %functions_namespace);

    let operator = Operator::new_with_check_functions_namespace(client, functions_namespace)
        .instrument(span)
        .await;

    operator.run().await;
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    init_tracing();
    OpenFaaSFunction::write_crds_to_file("crds.yaml");

    let functions_namespace = start_up();

    let client = KubeClient::try_default().await?;

    create_and_run_operator(client, functions_namespace)
        .instrument(trace_span!("Operator"))
        .await;

    Ok(())
}
