use std::path::PathBuf;

use crate::consts::{FUNCTIONS_DEFAULT_NAMESPACE, FUNCTIONS_NAMESPACE_ENV_VAR};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Runs the OpenFaaS functions operator
    Run {
        /// The namespace for OpenFaaS functions
        #[clap(long, env = FUNCTIONS_NAMESPACE_ENV_VAR, default_value = FUNCTIONS_DEFAULT_NAMESPACE)]
        functions_namespace: String,
    },
    /// Custom definition resource (CRD) commands
    Crd {
        #[command(subcommand)]
        command: CrdCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum CrdCommands {
    /// Writes the CRDs to a file
    Write {
        /// The path to the file to write the CRDs to
        #[clap(short, long)]
        path: PathBuf,
    },
    /// Prints the CRDs to stdout
    Print {},
    /// Installs the CRDs to the cluster
    Install {},
    /// Uninstalls the CRDs from the cluster
    Uninstall {},
    /// Converts the CRDs to Kubernetes resources
    Convert {
        /// The path to the file to read the CRDs from
        #[clap(short, long)]
        crd_path: PathBuf,

        #[command(subcommand)]
        command: CrdConvertCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum CrdConvertCommands {
    /// Writes the Kubernetes resources to a file
    Write {
        /// The path to the file to write the Kubernetes resources to
        #[clap(short, long)]
        resource_path: PathBuf,
    },
    /// Prints the Kubernetes resources to stdout
    Print {},
}
