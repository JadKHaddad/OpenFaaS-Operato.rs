use std::path::PathBuf;

use anyhow::{Context, Result as AnyResult};
use clap::Parser;
use either::Either::Left;
use k8s_openapi::{
    api::{apps::v1::Deployment, core::v1::Service},
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::{DeleteParams, PostParams},
    runtime::{conditions, wait::await_condition},
    Api, Client as KubeClient, CustomResourceExt, ResourceExt,
};
use openfaas_functions_operato_rs::{
    cli::{
        Cli, Commands, CrdCommands, CrdConvertCommands, OperatorCommands, OperatorDeployCommands,
        OperatorSubCommands,
    },
    consts::DEFAULT_IMAGE,
    crds::defs::{OpenFaaSFunction, NAME},
    operator::{deplyoment::DeploymentBuilder, Operator, UpdateStrategy},
};
use tracing::{trace_span, Instrument};
use tracing_subscriber::EnvFilter;

fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "openfaas_functions_operato_rs=info");
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
        Commands::Operator { command } => match *command {
            OperatorCommands::Controller {
                functions_namespace,
                update_strategy,
                command,
            } => match command {
                OperatorSubCommands::Run {} => {
                    init_tracing();
                    let client = KubeClient::try_default().await?;

                    tracing::info!(%functions_namespace, %update_strategy, "Running with current config.");

                    create_and_run_operator(client, functions_namespace, update_strategy)
                        .instrument(trace_span!("Operator"))
                        .await;
                }
                OperatorSubCommands::Deploy {
                    app_name,
                    image_name,
                    image_version,
                    command,
                } => {
                    let image = if let Some(image_version) = image_version {
                        format!("{}:{}", DEFAULT_IMAGE, image_version)
                    } else {
                        image_name
                    };

                    let deployment_builder = DeploymentBuilder {
                        app_name,
                        namespace: functions_namespace.clone(),
                        image,
                        update_strategy,
                    };

                    let yaml = deployment_builder.to_yaml_string()?;

                    match command {
                        OperatorDeployCommands::Write { file } => {
                            tokio::fs::write(file, yaml)
                                .await
                                .context("Failed to write resources to file")?;
                        }
                        OperatorDeployCommands::Print {} => {
                            println!("{}", yaml);
                        }
                        OperatorDeployCommands::Install {} => {
                            unimplemented!("Installis not implemented yet");
                        }
                        OperatorDeployCommands::Uninstall {} => {
                            unimplemented!("Uninstall is not implemented yet");
                        }
                        OperatorDeployCommands::Update {} => {
                            unimplemented!("Update is not implemented yet");
                        }
                    }
                }
            },
            OperatorCommands::Client { .. } => {
                unimplemented!("Client mode is not implemented yet");
            }
        },
        Commands::Crd { command } => match command {
            CrdCommands::Write { file } => {
                write_crd_to_file(file).await?;
            }
            CrdCommands::Print {} => {
                print_crd()?;
            }
            CrdCommands::Install {} => {
                let client = KubeClient::try_default().await?;
                install_crd(client).await?;
            }
            CrdCommands::Uninstall {} => {
                let client = KubeClient::try_default().await?;
                uninstall_crd(client).await?;
            }
            CrdCommands::Update {} => {
                let client = KubeClient::try_default().await?;
                update_crd(client).await?;
            }
            CrdCommands::Convert { crd_file, command } => {
                let crd = read_crd_from_file(crd_file).await?;

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

async fn read_crd_from_file(path: PathBuf) -> AnyResult<OpenFaaSFunction> {
    let crds = tokio::fs::read_to_string(path)
        .await
        .context("Failed to read crd from file")?;
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

async fn write_crd_to_file(path: PathBuf) -> AnyResult<()> {
    let crds = generate_crd_yaml()?;
    tokio::fs::write(path, crds)
        .await
        .context("Failed to write crd to file")?;
    Ok(())
}

async fn install_crd(client: KubeClient) -> AnyResult<()> {
    let api = Api::<CustomResourceDefinition>::all(client);
    let _ = api
        .create(&PostParams::default(), &OpenFaaSFunction::crd())
        .await?;

    await_condition(api, NAME, conditions::is_crd_established()).await?;

    Ok(())
}

async fn uninstall_crd(client: KubeClient) -> AnyResult<()> {
    let api = Api::<CustomResourceDefinition>::all(client);

    let obj = api.delete(NAME, &Default::default()).await?;
    if let Left(o) = obj {
        match o.uid() {
            Some(uid) => {
                await_condition(api, NAME, conditions::is_deleted(&uid)).await?;
            }
            None => {
                tracing::warn!("Could not find crd's uid");
            }
        }
    }

    Ok(())
}

async fn update_crd(_client: KubeClient) -> AnyResult<()> {
    unimplemented!("Update is not implemented yet")
}
