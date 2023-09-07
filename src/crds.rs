use kube::CustomResource;
use kube::CustomResourceExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const FINALIZER_NAME: &str = "openfaasfunctions.operato.rs/finalizer";

// TODO: constraints will be sent to the faas-provider, so we need the faas_client

#[derive(CustomResource, Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[kube(
    group = "operato.rs",
    version = "v1",
    kind = "OpenFaaSFunction",
    plural = "openfaasfunctions",
    derive = "PartialEq",
    status = "OpenFaasFunctionStatus",
    namespaced
)]
#[serde(rename_all = "camelCase")]
pub struct OpenFaasFunctionSpec {
    /// service is the name of the function deployment
    pub service: String,

    /// image is a fully-qualified container image
    pub image: String,

    /// namespace for the function, if supported by the faas-provider
    pub namespace: Option<String>,

    /// envProcess overrides the fprocess environment variable and can be used
    /// with the watchdog
    pub env_process: Option<String>,

    /// envVars can be provided to set environment variables for the function runtime.
    pub env_vars: Option<HashMap<String, String>>,

    /// constraints are specific to the faas-provider.
    pub constraints: Option<Vec<String>>,

    /// secrets list of secrets to be made available to function
    pub secrets: Option<Vec<String>>,

    /// labels are metadata for functions which may be used by the
    /// faas-provider or the gateway
    pub labels: Option<HashMap<String, String>>,

    /// annotations are metadata for functions which may be used by the
    /// faas-provider or the gateway
    pub annotations: Option<HashMap<String, String>>,

    /// limits for function
    pub limits: Option<FunctionResources>,

    /// requests of resources requested by function
    pub requests: Option<FunctionResources>,

    /// readOnlyRootFilesystem removes write-access from the root filesystem
    /// mount-point.
    pub read_only_root_filesystem: Option<bool>,
}

/// FunctionResources Memory and CPU
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, JsonSchema)]
pub struct FunctionResources {
    /// memory is the memory limit for the function
    pub memory: Option<String>,
    /// cpu is the cpu limit for the function
    pub cpu: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
pub enum OpenFaasFunctionStatus {
    Deployed,
    InvalidCRDNamespace,
    InvalidFunctionNamespace,
}

impl OpenFaaSFunction {
    pub fn generate_crds() -> String {
        serde_yaml::to_string(&OpenFaaSFunction::crd()).expect("Failed to generate crds")
    }

    pub fn print_crds() {
        println!("{:#?}", OpenFaaSFunction::generate_crds());
    }

    pub fn write_crds_to_file(path: &str) {
        let crds = OpenFaaSFunction::generate_crds();
        std::fs::write(path, crds).expect("Failed to write crds to file");
    }
}
