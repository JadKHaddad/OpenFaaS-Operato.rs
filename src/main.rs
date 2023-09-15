use std::path::PathBuf;

use anyhow::{Context, Result as AnyResult};
use clap::Parser;
use k8s_openapi::apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition;
use kube::{
    api::{DeleteParams, PostParams},
    Api, Client as KubeClient, CustomResourceExt,
};
use openfaas_functions_operato_rs::{
    cli::{Cli, Commands, CrdCommands, CrdConvertCommands},
    crds::defs::OpenFaaSFunction,
    operator::Operator,
};
use tracing::{trace_span, Instrument};
use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "openfaas_operato_rs=debug");
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
    let cli = Cli::parse();

    tracing::debug!("{:#?}", cli);

    match cli.command {
        Commands::Run {
            functions_namespace,
        } => {
            let client = KubeClient::try_default().await?;

            create_and_run_operator(client, functions_namespace)
                .instrument(trace_span!("Operator"))
                .await;
        }
        Commands::Crd { command } => match command {
            CrdCommands::Write { path } => {
                write_crd_to_file(path)?;
            }
            CrdCommands::Print {} => {
                print_crd()?;
            }
            CrdCommands::Install {} => {
                let client = KubeClient::try_default().await?;

                let api = Api::<CustomResourceDefinition>::all(client);
                if let Err(error) = api
                    .create(&PostParams::default(), &OpenFaaSFunction::crd())
                    .await
                {
                    tracing::error!(%error, "Failed to install CRD");
                }
            }
            CrdCommands::Uninstall {} => {
                let client = KubeClient::try_default().await?;

                let api = Api::<CustomResourceDefinition>::all(client);
                if let Err(error) = api
                    .delete(OpenFaaSFunction::crd_name(), &DeleteParams::default())
                    .await
                {
                    tracing::error!(%error, "Failed to uninstall CRD");
                }
            }
            CrdCommands::Convert { crd_path, command } => {
                let crd = read_crd_from_file(crd_path)?;
                match crd.spec.to_yaml_string() {
                    Err(error) => {
                        tracing::error!(%error, "Failed to convert crd to yaml");
                    }
                    Ok(yaml) => match command {
                        CrdConvertCommands::Write { resource_path } => {
                            std::fs::write(resource_path, yaml)
                                .context("Failed to write crd to file")?;
                        }
                        CrdConvertCommands::Print {} => {
                            println!("{}", yaml);
                        }
                    },
                }
            }
        },
    }

    Ok(())
}

pub fn read_crd_from_file(path: PathBuf) -> AnyResult<OpenFaaSFunction> {
    let crds = std::fs::read_to_string(path).context("Failed to read crd from file")?;
    let crd = serde_yaml::from_str(&crds).context("Failed to parse crd")?;
    Ok(crd)
}

pub fn generate_crd_yaml() -> AnyResult<String> {
    serde_yaml::to_string(&OpenFaaSFunction::crd()).context("Failed to generate crd")
}

pub fn print_crd() -> AnyResult<()> {
    println!("{}", generate_crd_yaml()?);
    Ok(())
}

pub fn write_crd_to_file(path: PathBuf) -> AnyResult<()> {
    let crds = generate_crd_yaml()?;
    std::fs::write(path, crds).context("Failed to write crd to file")?;
    Ok(())
}
