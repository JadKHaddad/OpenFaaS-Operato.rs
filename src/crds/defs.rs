use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error as ThisError;

pub const FINALIZER_NAME: &str = "openfaasfunctions.operato.rs/finalizer";

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
    Ready,
    Deployed,
    InvalidCRDNamespace,
    InvalidFunctionNamespace,
}

pub enum DeploymentDiff {
    /// ```Container``` is missing. Name: ```OpenFaasFunctionSpec::service```
    Container,
    /// If ```Container``` is not missing in the deployment containers, but ```Image``` is different
    Image,
    /// ```EnvProcess``` is missing or different
    EnvProcess,
    /// ```EnvVars``` are missing
    NoEnvVars,
    /// An ```EnvVar``` is missing or different
    EnvVar(String),
    /// ```Constraints``` are missing
    NoConstraints,
    /// A ```Constraint``` is missing
    Constraints(String),
    /// ```Secrets``` are missing
    NoSecrets,
    /// A ```Secret``` is missing or different
    Secrets(String),
    /// ```Labels``` are missing
    NoLabels,
    /// A ```Label``` is missing or different
    Labels(String),
    /// ```Annotations``` are missing
    NoAnnotations,
    /// An ```Annotation``` is missing or different
    Annotation(String),
    /// ```Limits``` are missing or different
    Limits(ResourceDiff),
    /// ```Requests``` are missing or different
    Requests(ResourceDiff),
    /// ```ReadOnlyRootFilesystem``` is missing or different
    ReadOnlyRootFilesystem,
}

pub enum ResourceDiff {
    Memory,
    CPU,
}

pub enum ServiceDiff {}

#[derive(ThisError, Debug)]
pub enum IntoDeploymentError {
    #[error("Failed to get owner reference")]
    FailedToGetOwnerReference,
}

#[derive(ThisError, Debug)]
pub enum IntoServiceError {
    #[error("Failed to get owner reference")]
    FailedToGetOwnerReference,
}
