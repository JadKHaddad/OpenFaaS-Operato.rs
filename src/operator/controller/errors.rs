use crate::crds::defs::{
    FunctionIntoDeploymentError, FunctionIntoServiceError, OpenFaasFunctionPossibleStatus,
};
use kube::Error as KubeError;
use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum ReconcileError {
    #[error("Resource has no namespace.")]
    Namespace,
    #[error("Failed to apply resource: {0}")]
    Apply(#[source] ApplyError),
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
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}

#[derive(ThisError, Debug)]
pub enum CheckFunctionNamespaceError {
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}

#[derive(ThisError, Debug)]
#[error("Failed to set satus to {status:?}: {error}")]
pub struct StatusError {
    #[source]
    pub error: SetStatusError,
    pub status: OpenFaasFunctionPossibleStatus,
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
    #[error("Error listing secrets: {0}")]
    List(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}

#[derive(ThisError, Debug)]
pub enum DeploymentError {
    #[error("Failed to get deployment: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to create deployment: {0}")]
    Create(#[source] CreateDeploymentError),
    #[error("Failed to check deployment: {0}")]
    Check(#[source] CheckDeploymentError),
    #[error("Failed to delete deployment: {0}")]
    Delete(#[source] DeleteDeploymentsError),
}

#[derive(ThisError, Debug)]
pub enum CheckDeploymentError {
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
    #[error("Failed to create deployment: {0}")]
    Create(#[source] CreateDeploymentError),
}

#[derive(ThisError, Debug)]
pub enum CreateDeploymentError {
    #[error("Failed to check secrets: {0}")]
    Secrets(#[source] CheckSecretsError),
    #[error("Failed to generate deployment: {0}")]
    Generate(#[source] FunctionIntoDeploymentError),
    #[error("Failed to apply deployment: {0}")]
    Apply(#[source] KubeError),
    #[error("Failed to replace deployment: {0}")]
    Replace(#[source] KubeError),
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}

#[derive(ThisError, Debug)]
pub enum DeleteDeploymentsError {
    #[error("Error listing deployments: {0}")]
    List(#[source] KubeError),
    #[error("Error deleting deployment: {0}")]
    Delete(#[source] KubeError),
}

#[derive(ThisError, Debug)]
pub enum ServiceError {
    #[error("Failed to get service: {0}")]
    Get(#[source] KubeError),
    #[error("Failed to get owner reference")]
    OwnerReference,
    #[error("Failed to create service: {0}")]
    Create(#[source] CreateServiceError),
    #[error("Failed to check service: {0}")]
    Check(#[source] CheckServiceError),
    #[error("Failed to delete service: {0}")]
    Delete(#[source] DeleteServicesError),
}

#[derive(ThisError, Debug)]
pub enum CreateServiceError {
    #[error("Failed to generate deployment: {0}")]
    Generate(#[source] FunctionIntoServiceError),
    #[error("Failed to apply deployment: {0}")]
    Apply(#[source] KubeError),
}

#[derive(ThisError, Debug)]
pub enum CheckServiceError {
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}

#[derive(ThisError, Debug)]
pub enum DeleteServicesError {
    #[error("Error listing services: {0}")]
    List(#[source] KubeError),
    #[error("Error deleting service: {0}")]
    Delete(#[source] KubeError),
}

#[derive(ThisError, Debug)]
pub enum DeployedStatusError {
    #[error("Error getting status: {0}")]
    GetStatus(#[source] KubeError),
    #[error("Error setting status: {0}")]
    SetStatus(#[source] StatusError),
}
