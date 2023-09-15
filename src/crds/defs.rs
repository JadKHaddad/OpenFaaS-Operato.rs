use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::CustomResource;
use kube_quantity::ParseQuantityError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error as ThisError;

pub const FINALIZER_NAME: &str = "openfaasfunctions.operato.rs/finalizer";
pub const LAST_APPLIED_ANNOTATION: &str = "openfaasfunctions.operato.rs/last-applied-spec";

#[derive(CustomResource, Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
#[kube(
    group = "operato.rs",
    version = "v1alpha1",
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

    /// namespace for the function
    pub namespace: Option<String>,

    /// envProcess overrides the fprocess environment variable and can be used
    /// with the watchdog
    pub env_process: Option<String>,

    /// envVars can be provided to set environment variables for the function runtime
    pub env_vars: Option<HashMap<String, String>>,

    /// constraints are specific to the faas-provider.
    pub constraints: Option<Vec<String>>,

    /// list of names of secrets in the same namespace that will be mounted to secretsMountPath
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

    /// secretsMountPath is the path where secrets will be mounted
    /// defaults to /var/openfaas/secrets
    pub secrets_mount_path: Option<String>,
}

/// FunctionResources Memory and CPU
/// Must match ^([+-]?[0-9.]+)([eEinumkKMGTP][-+]?[0-9])$
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, JsonSchema)]
pub struct FunctionResources {
    /// memory is the memory limit for the function
    pub memory: Option<String>,
    /// cpu is the cpu limit for the function
    pub cpu: Option<String>,
}

pub struct FunctionResourcesQuantity {
    pub memory: Option<Quantity>,
    pub cpu: Option<Quantity>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
pub enum OpenFaasFunctionStatus {
    Ok(OpenFaasFunctionOkStatus),
    Err(OpenFaasFunctionErrorStatus),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema)]
pub enum OpenFaasFunctionOkStatus {
    Ready,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, JsonSchema, ThisError)]
pub enum OpenFaasFunctionErrorStatus {
    #[error("The CRD's namespace does not match the functions namespace")]
    InvalidCRDNamespace,
    #[error("The function's namespace does not match the functions namespace")]
    InvalidFunctionNamespace,
    #[error("A function's cpu quantity is invalid")]
    CPUQuantity,
    #[error("A function's memory quantity is invalid")]
    MemoryQuantity,
    #[error("The function's deployment already deployed by third party")]
    DeploymentAlreadyExists,
    #[error("The function's deployment is not ready")]
    DeploymentNotReady,
    #[error("The function's service already deployed by third party")]
    ServiceAlreadyExists,
    #[error("The given secrets to mount do not exist")]
    SecretsNotFound,
}

#[derive(ThisError, Debug)]
pub enum FunctionSpecIntoYamlError {
    #[error("Failed to generate deployment: {0}")]
    Deployment(FunctionSpecIntoDeploymentError),
    #[error("Failed to generate service: {0}")]
    Service(FunctionSpecIntoServiceError),
    #[error("Failed to serialize: {0}")]
    Serialize(
        #[source]
        #[from]
        serde_yaml::Error,
    ),
}

#[derive(ThisError, Debug)]
pub enum FunctionIntoDeploymentError {
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate deployment from spec: {0}")]
    FunctionSpec(
        #[source]
        #[from]
        FunctionSpecIntoDeploymentError,
    ),
}

#[derive(ThisError, Debug)]
pub enum FunctionSpecIntoDeploymentError {
    #[error("Faild to serialize: {0}")]
    Serialize(
        #[source]
        #[from]
        serde_json::Error,
    ),
    #[error("Failed to parse quantity: {0} | Quantity must match ^([+-]?[0-9.]+)([eEinumkKMGTP][-+]?[0-9])$")]
    Quantity(
        #[source]
        #[from]
        IntoQuantityError,
    ),
}

#[derive(ThisError, Debug)]
pub enum FunctionIntoServiceError {
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate service from spec: {0}")]
    FunctionSpec(
        #[source]
        #[from]
        FunctionSpecIntoServiceError,
    ),
}

#[derive(ThisError, Debug)]
pub enum FunctionSpecIntoServiceError {
    #[error("Faild to serialize: {0}")]
    Serialize(
        #[source]
        #[from]
        serde_json::Error,
    ),
}

#[derive(ThisError, Debug)]
pub enum IntoQuantityError {
    #[error("Failed to parse cpu quantity: {0}")]
    CPU(#[source] ParseQuantityError),
    #[error("Failed to parse memory quantity: {0}")]
    Memory(#[source] ParseQuantityError),
}
