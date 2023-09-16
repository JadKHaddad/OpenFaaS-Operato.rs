use std::path::PathBuf;

use anyhow::{Context, Result as AnyResult};
use clap::Parser;
use k8s_openapi::{
    api::{apps::v1::Deployment, core::v1::Service},
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::{DeleteParams, PostParams},
    Api, Client as KubeClient, CustomResourceExt,
};
use openfaas_functions_operato_rs::{
    cli::{Cli, Commands, CrdCommands, CrdConvertCommands, RunCommands},
    crds::defs::OpenFaaSFunction,
    operator::{Operator, UpdateStrategy},
};
use tracing::{trace_span, Instrument};
use tracing_subscriber::EnvFilter;

fn init_tracing() {
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

async fn create_and_run_operator(
    client: KubeClient,
    functions_namespace: String,
    update_strategy: UpdateStrategy,
) {
    let span = trace_span!("Create", %functions_namespace);

    let operator =
        Operator::new_with_check_functions_namespace(client, functions_namespace, update_strategy)
            .instrument(span)
            .await;

    operator.run().await;
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { command } => {
            init_tracing();
            let client = KubeClient::try_default().await?;

            match command {
                RunCommands::Controller {
                    functions_namespace,
                    update_strategy,
                } => {
                    create_and_run_operator(client, functions_namespace, update_strategy)
                        .instrument(trace_span!("Operator"))
                        .await;
                }
                RunCommands::Client { .. } => {
                    tracing::warn!("Client mode is not implemented yet");
                }
            }
        }
        Commands::Crd { command } => match command {
            CrdCommands::Write { file } => {
                write_crd_to_file(file)?;
            }
            CrdCommands::Print {} => {
                print_crd()?;
            }
            CrdCommands::Install {} => {
                let client = KubeClient::try_default().await?;

                let api = Api::<CustomResourceDefinition>::all(client);
                let _ = api
                    .create(&PostParams::default(), &OpenFaaSFunction::crd())
                    .await?;
            }
            CrdCommands::Uninstall {} => {
                let client = KubeClient::try_default().await?;

                let api = Api::<CustomResourceDefinition>::all(client);
                let _ = api
                    .delete(OpenFaaSFunction::crd_name(), &DeleteParams::default())
                    .await?;
            }
            CrdCommands::Convert { crd_file, command } => {
                let crd = read_crd_from_file(crd_file)?;

                match command {
                    CrdConvertCommands::Write { resource_file } => {
                        let yaml = crd.spec.to_yaml_string()?;
                        std::fs::write(resource_file, yaml)
                            .context("Failed to write crd to file")?;
                    }
                    CrdConvertCommands::Print {} => {
                        let yaml = crd.spec.to_yaml_string()?;
                        println!("{}", yaml);
                    }
                    CrdConvertCommands::Apply {} => {
                        let client = KubeClient::try_default().await?;

                        let deployment_api = Api::<Deployment>::all(client.clone());
                        let service_api = Api::<Service>::all(client);

                        let deployment = Deployment::try_from(&crd.spec)?;
                        let service = Service::try_from(&crd.spec)?;

                        deployment_api
                            .create(&PostParams::default(), &deployment)
                            .await?;

                        service_api.create(&PostParams::default(), &service).await?;
                    }
                    CrdConvertCommands::Delete {} => {
                        let client = KubeClient::try_default().await?;

                        let deployment_api = Api::<Deployment>::all(client.clone());
                        let service_api = Api::<Service>::all(client);

                        let name = crd.spec.to_name();

                        deployment_api
                            .delete(&name, &DeleteParams::default())
                            .await?;

                        service_api.delete(&name, &DeleteParams::default()).await?;
                    }
                }
            }
        },
    }

    Ok(())
}

fn read_crd_from_file(path: PathBuf) -> AnyResult<OpenFaaSFunction> {
    let crds = std::fs::read_to_string(path).context("Failed to read crd from file")?;
    let crd = serde_yaml::from_str(&crds).context("Failed to parse crd")?;
    Ok(crd)
}

fn generate_crd_yaml() -> AnyResult<String> {
    serde_yaml::to_string(&OpenFaaSFunction::crd()).context("Failed to generate crd")
}

fn print_crd() -> AnyResult<()> {
    println!("{}", generate_crd_yaml()?);
    Ok(())
}

fn write_crd_to_file(path: PathBuf) -> AnyResult<()> {
    let crds = generate_crd_yaml()?;
    std::fs::write(path, crds).context("Failed to write crd to file")?;
    Ok(())
}
