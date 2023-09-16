use std::path::PathBuf;

use crate::{
    consts::{
        FUNCTIONS_DEFAULT_NAMESPACE, FUNCTIONS_NAMESPACE_ENV_VAR, GATEWAY_DEFAULT_URL,
        GATEWAY_URL_ENV_VAR, OPFOC_UPDATE_STRATEGY_ENV_VAR,
    },
    operator::UpdateStrategy,
};
use clap::{Parser, Subcommand};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Runs the OpenFaaS functions operator
    #[clap(visible_alias = "r")]
    Run {
        #[command(subcommand)]
        command: RunCommands,
    },
    /// Custom definition resource (CRD) commands
    #[clap(visible_alias = "c")]
    Crd {
        #[command(subcommand)]
        command: CrdCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum RunCommands {
    /// Runs the OpenFaaS functions operator in controller mode
    #[clap(visible_alias = "co")]
    Controller {
        /// The namespace for OpenFaaS functions
        #[clap(short = 'n', long, env = FUNCTIONS_NAMESPACE_ENV_VAR, default_value = FUNCTIONS_DEFAULT_NAMESPACE)]
        functions_namespace: String,
        /// Update strategy for the operator
        #[clap(short, long, env = OPFOC_UPDATE_STRATEGY_ENV_VAR, value_enum, default_value_t = UpdateStrategy::default())]
        update_strategy: UpdateStrategy,
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
    },
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
    /// This is equivalent to uninstalling and then installing the CRDs
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
