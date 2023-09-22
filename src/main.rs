use anyhow::{Context, Ok, Result as AnyResult};
use clap::Parser;
#[cfg(debug_assertions)]
use openfaas_functions_operato_rs::cli::DockerCommands;
#[cfg(debug_assertions)]
use openfaas_functions_operato_rs::docker_actions::*;
use openfaas_functions_operato_rs::main_actions::*;
use openfaas_functions_operato_rs::{
    cli::{
        Cli, Commands, CrdCommands, CrdConvertCommands, OperatorCommands, OperatorDeployCommands,
        OperatorSubCommands,
    },
    consts::PKG_NAME,
    operator::controller::deplyoment::DeploymentBuilder,
};
use tracing::{trace_span, Instrument};
use tracing_subscriber::EnvFilter;

fn init_tracing() {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", format!("{PKG_NAME}=info,kube=off"));
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

#[tokio::main]
async fn main() -> AnyResult<()> {
    let cli = Cli::parse();

    init_tracing();

    match cli.command {
        Commands::Operator { command } => match *command {
            OperatorCommands::Controller {
                functions_namespace,
                update_strategy,
                command,
            } => match command {
                OperatorSubCommands::Run {} => {
                    print_disply_name();

                    create_and_run_operator_controller(functions_namespace, update_strategy)
                        .instrument(trace_span!("Operator"))
                        .await?;
                }
                OperatorSubCommands::Deploy {
                    app_name,
                    image_name,
                    image_version,
                    command,
                } => {
                    let image = determin_image(image_name, image_version);

                    let deployment_builder = DeploymentBuilder::new(
                        app_name,
                        functions_namespace.clone(),
                        image,
                        update_strategy,
                    );

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
                            install_operator_controller(deployment_builder, functions_namespace)
                                .await?
                        }
                        OperatorDeployCommands::Uninstall {} => {
                            uninstall_operator_controller(deployment_builder, functions_namespace)
                                .await?
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
            CrdCommands::Print {} => print_crd()?,
            CrdCommands::Install {} => {
                install_crd().await?;
            }
            CrdCommands::Uninstall {} => {
                uninstall_crd().await?;
            }
            CrdCommands::Update {} => unimplemented!("Update is not implemented yet"),
            CrdCommands::Convert { crd_file, command } => {
                let crd = read_crd_from_file(crd_file).await?;
                match command {
                    CrdConvertCommands::Write { resource_file } => {
                        write_crd_resources_to_file(resource_file, crd).await?
                    }
                    CrdConvertCommands::Print {} => print_crd_resources(crd)?,
                    CrdConvertCommands::Apply {} => apply_crd_resources(crd).await?,
                    CrdConvertCommands::Delete {} => delete_crd_resources(crd).await?,
                }
            }
        },
        #[cfg(debug_assertions)]
        Commands::Docker {
            accept,
            context,
            dockerfile,
            image_name,
            use_package_version,
            command,
        } => {
            let image_name = determin_image_for_build(image_name, use_package_version);
            let message = format!("You are about to build the image: {}", image_name);

            if !accept && !are_you_sure_you_want_to_run_this_command(&message)? {
                return Ok(());
            }

            match command {
                DockerCommands::Build {} => build(context, dockerfile, image_name).await?,
                DockerCommands::Push {} => push(image_name).await?,
                DockerCommands::Up {} => build_and_push(context, dockerfile, image_name).await?,
            }
        }
    }

    Ok(())
}
