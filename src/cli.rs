use crate::{
    consts::{
        DEFAULT_IMAGE_WITH_TAG, FUNCTIONS_DEFAULT_NAMESPACE, FUNCTIONS_NAMESPACE_ENV_VAR,
        GATEWAY_DEFAULT_URL, GATEWAY_URL_ENV_VAR, OPFOC_UPDATE_STRATEGY_ENV_VAR, PKG_VERSION,
    },
    crds::defs::VERSION as CRD_VERSION,
    operator::controller::UpdateStrategy,
};
use clap::{Parser, Subcommand};
use const_format::formatcp;
use std::path::PathBuf;
use url::Url;

const VERSION: &str = formatcp!("{0}, crd {1}", PKG_VERSION, CRD_VERSION);

#[cfg(test)]
const NO_BINARY_NAME: bool = true;
#[cfg(not(test))]
const NO_BINARY_NAME: bool = false;

#[derive(Parser, Debug)]
#[command(author, version=VERSION, about, long_about = None, no_binary_name(NO_BINARY_NAME))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub fn operator_controller_run_args(
        namesapce: String,
        update_strategy: UpdateStrategy,
    ) -> Vec<String> {
        vec![
            String::from("operator"),
            String::from("controller"),
            String::from("--functions-namespace"),
            namesapce,
            String::from("--update-strategy"),
            update_strategy.to_string(),
            String::from("run"),
        ]
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Operator commands
    #[clap(visible_alias = "o")]
    Operator {
        #[command(subcommand)]
        command: Box<OperatorCommands>,
    },
    /// Custom definition resource (CRD) commands
    #[clap(visible_alias = "c")]
    Crd {
        #[command(subcommand)]
        command: CrdCommands,
    },
    /// Docker command
    ///
    /// Builds and pushes the Docker image for the OpenFaaS functions operator
    Docker {
        /// The name of the image
        #[clap(short = 'i', long, default_value = DEFAULT_IMAGE_WITH_TAG)]
        image_name: String,
        /// Context path for the Docker build
        #[clap(short = 'c', long, default_value = ".")]
        context: PathBuf,
        /// The name of the Dockerfile to use
        #[clap(short = 'f', long, default_value = "Dockerfile")]
        dockerfile: PathBuf,

        #[command(subcommand)]
        command: DockerCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum DockerCommands {
    /// Builds the Docker image
    Build {},
    /// Pushes the Docker image
    Push {},
    /// Builds and pushes the Docker image
    Up {},
}

#[derive(Subcommand, Debug)]
pub enum OperatorCommands {
    /// Runs the OpenFaaS functions operator in controller mode
    #[clap(visible_alias = "co")]
    Controller {
        /// The namespace for OpenFaaS functions
        #[clap(short = 'n', long, env = FUNCTIONS_NAMESPACE_ENV_VAR, default_value = FUNCTIONS_DEFAULT_NAMESPACE)]
        functions_namespace: String,
        /// Update strategy for the operator
        #[clap(short, long, env = OPFOC_UPDATE_STRATEGY_ENV_VAR, value_enum, default_value_t = UpdateStrategy::default())]
        update_strategy: UpdateStrategy,

        #[command(subcommand)]
        command: OperatorSubCommands,
    },
    /// Runs the OpenFaaS functions operator in client mode
    #[clap(visible_alias = "cl")]
    Client {
        /// The URL of the OpenFaaS gateway
        #[clap(short, long, env = GATEWAY_URL_ENV_VAR, default_value = GATEWAY_DEFAULT_URL)]
        gateway_url: Url,
        /// The username for the OpenFaaS gateway
        #[clap(short, long)]
        username: Option<String>,
        /// The password for the OpenFaaS gateway
        #[clap(short, long)]
        password: Option<String>,
        /// The path to a file containing the username for the OpenFaaS gateway
        /// If this is set, the username argument is ignored
        #[clap(long)]
        username_file: Option<PathBuf>,
        /// The path to a file containing the password for the OpenFaaS gateway
        /// If this is set, the password argument is ignored
        #[clap(long)]
        password_file: Option<PathBuf>,

        #[command(subcommand)]
        command: OperatorSubCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum OperatorSubCommands {
    /// Runs the OpenFaaS functions operator
    #[clap(visible_alias = "r")]
    Run {},
    /// Generates the Kubernetes resources for the OpenFaaS functions operator
    #[clap(visible_alias = "d")]
    Deploy {
        /// The name of the OpenFaaS functions operator
        #[clap(short, long, default_value = "openfaas-functions-operator")]
        app_name: String,
        /// The name of the image to use for the OpenFaaS functions operator
        #[clap(short = 'i', long, default_value = DEFAULT_IMAGE_WITH_TAG)]
        image_name: String,
        /// The version of the image to use for the OpenFaaS functions operator
        /// If this is set, the image_name argument is ignored, and the image_name is set to the default image
        #[clap(short = 'v', long)]
        image_version: Option<String>,

        #[command(subcommand)]
        command: OperatorDeployCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum OperatorDeployCommands {
    /// Writes the Kubernetes resources to a file
    #[clap(visible_alias = "w")]
    Write {
        /// The path to the file to write the Kubernetes resources to
        #[clap(short, long)]
        file: PathBuf,
    },
    /// Prints the Kubernetes resources to stdout
    #[clap(visible_alias = "p")]
    Print {},
    /// Applies the Kubernetes resources to the cluster
    #[clap(visible_alias = "in")]
    Install {},
    /// Deletes the Kubernetes resources from the cluster
    #[clap(visible_alias = "un")]
    Uninstall {},
    /// Updates the Kubernetes resources in the cluster
    #[clap(visible_alias = "up")]
    Update {},
}

#[derive(Subcommand, Debug)]
pub enum CrdCommands {
    /// Writes the CRDs to a file
    #[clap(visible_alias = "w")]
    Write {
        /// The path to the file to write the CRDs to
        #[clap(short, long)]
        file: PathBuf,
    },
    /// Prints the CRDs to stdout
    #[clap(visible_alias = "p")]
    Print {},
    /// Installs the CRDs to the cluster
    #[clap(visible_alias = "in")]
    Install {},
    /// Uninstalls the CRDs from the cluster
    #[clap(visible_alias = "un")]
    Uninstall {},
    /// Updates the CRDs in the cluster
    #[clap(visible_alias = "up")]
    Update {},
    /// Converts the CRDs to Kubernetes resources
    #[clap(visible_alias = "c")]
    Convert {
        /// The path to the file to read the CRDs from
        #[clap(short = 'f', long)]
        crd_file: PathBuf,

        #[command(subcommand)]
        command: CrdConvertCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum CrdConvertCommands {
    /// Writes the Kubernetes resources to a file
    #[clap(visible_alias = "w")]
    Write {
        /// The path to the file to write the Kubernetes resources to
        #[clap(short = 'f', long)]
        resource_file: PathBuf,
    },
    /// Prints the Kubernetes resources to stdout
    #[clap(visible_alias = "p")]
    Print {},
    /// Applies the Kubernetes resources to the cluster
    /// No guarantees or checks are made to ensure the resources are applied correctly
    #[clap(visible_alias = "a")]
    Apply {},
    /// Deletes the Kubernetes resources from the cluster
    #[clap(visible_alias = "d")]
    Delete {},
}

// https://docs.rs/clap/latest/clap/_derive/index.html#arg-attributes

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn operator_controller_run_args_are_valid() {
        let namespace_arg = String::from("functions");
        let update_strategy_arg = UpdateStrategy::OneWay;

        let args =
            Cli::operator_controller_run_args(namespace_arg.clone(), update_strategy_arg.clone());

        let cli = Cli::parse_from(args);

        if let Commands::Operator { command } = cli.command {
            if let OperatorCommands::Controller {
                functions_namespace,
                update_strategy,
                command: OperatorSubCommands::Run {},
            } = *command
            {
                assert_eq!(functions_namespace, namespace_arg);
                assert_eq!(update_strategy, update_strategy_arg);
                return;
            }
        }

        panic!("Operator controller run args are invalid");
    }
}
