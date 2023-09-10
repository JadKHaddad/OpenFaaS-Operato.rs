use crate::crds::defs::{IntoDeploymentError, IntoServiceError, OpenFaasFunctionStatus};
use kube::{runtime::finalizer::Error as FinalizerError, Error as KubeError};
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum ReconcileError {
    #[error("Resource has no namespace.")]
    Namespace,
    #[error("Failed to finalize resource: {0}")]
    FinalizeError(#[source] FinalizerError<FinalizeError>),
}

#[derive(ThisError, Debug)]
pub enum ApplyError {
    #[error("Failed to check resource namespace: {0}")]
    ResourceNamespace(#[source] CheckResourceNamespaceError),
    #[error("Failed to check function namespace: {0}")]
    FunctionNamespace(#[source] CheckFunctionNamespaceError),
    #[error("Deployment error: {0}")]
    Deployment(#[source] DeploymentError),
    #[error("Service error: {0}")]
    Service(#[source] ServiceError),
    #[error("Status error: {0}")]
    Status(#[source] DeployedStatusError),
}

#[derive(ThisError, Debug)]
pub enum CheckResourceNamespaceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
pub enum CheckFunctionNamespaceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
#[error("Failed to set satus to {status:?}: {error}")]
pub struct StatusError {
    #[source]
    pub error: SetStatusError,
    pub status: OpenFaasFunctionStatus,
}

#[derive(ThisError, Debug)]
pub enum SetStatusError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to serialize resource.")]
    Serilization(#[source] serde_json::Error),
}

#[derive(ThisError, Debug)]
pub enum CheckSecretsError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
pub enum DeploymentError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to get deployment: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to check secrets: {0}")]
    Secrets(#[source] CheckSecretsError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate deployment: {0}")]
    Generate(#[source] IntoDeploymentError),
    #[error("Failed to apply deployment: {0}")]
    Apply(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
pub enum ServiceError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error("Failed to get service: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to generate service: {0}")]
    Generate(#[source] IntoServiceError),
    #[error("Failed to apply service: {0}")]
    Apply(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
pub enum DeployedStatusError {
    #[error("Kubernetes error: {0}")]
    Kube(#[source] KubeError),
    #[error(transparent)]
    Status(StatusError),
}

#[derive(ThisError, Debug)]
pub enum CleanupError {}

#[derive(ThisError, Debug)]
pub enum FinalizeError {
    #[error("Failed to apply resource: {0}")]
    Apply(#[source] ApplyError),

    #[error("Failed to cleanup resource: {0}")]
    Cleanup(#[source] CleanupError),
}
