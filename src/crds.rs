use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::apps::v1::DeploymentSpec;
use k8s_openapi::api::core::v1::Container;
use k8s_openapi::api::core::v1::ContainerPort;
use k8s_openapi::api::core::v1::HTTPGetAction;
use k8s_openapi::api::core::v1::PodSpec;
use k8s_openapi::api::core::v1::PodTemplateSpec;
use k8s_openapi::api::core::v1::Probe;
use k8s_openapi::api::core::v1::SecurityContext;
use k8s_openapi::api::core::v1::Service;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::LabelSelector;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::core::ObjectMeta;
use kube::CustomResource;
use kube::CustomResourceExt;
use kube::Resource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

impl OpenFaasFunctionSpec {
    pub fn deployment_diffs(&self, deployment: &Deployment) -> Vec<DeploymentDiff> {
        unimplemented!()
    }

    pub fn service_diffs(&self, service: &Service) -> Vec<ServiceDiff> {
        unimplemented!()
    }

    fn to_name(&self) -> String {
        self.service.clone()
    }

    fn to_namespace(&self) -> Option<String> {
        self.namespace.clone()
    }

    fn to_image(&self) -> String {
        self.image.clone()
    }

    fn to_meta_labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::new();
        labels.insert(String::from("faas_function"), self.service.clone());
        labels
    }

    fn to_spec_meta_labels(&self) -> BTreeMap<String, String> {
        let meta_labels = self.to_meta_labels();
        if let Some(lables) = self.labels.clone() {
            let mut labels: BTreeMap<String, String> = lables.into_iter().collect();
            labels.extend(meta_labels);

            labels
        } else {
            meta_labels
        }
    }

    fn to_annotations(&self) -> Option<BTreeMap<String, String>> {
        if let Some(annotations) = self.annotations.clone() {
            let annotations: BTreeMap<String, String> = annotations.into_iter().collect();
            Some(annotations)
        } else {
            None
        }
    }

    fn to_deployment_meta(&self) -> ObjectMeta {
        ObjectMeta {
            name: Some(self.to_name()),
            namespace: self.to_namespace(),
            labels: Some(self.to_meta_labels()),
            annotations: self.to_annotations(),
            ..Default::default()
        }
    }

    fn to_spec_template_meta(&self) -> ObjectMeta {
        ObjectMeta {
            name: Some(self.to_name()),
            labels: Some(self.to_spec_meta_labels()),
            annotations: self.to_annotations(),
            ..Default::default()
        }
    }

    fn to_container_ports(&self) -> Vec<ContainerPort> {
        vec![ContainerPort::from(self)]
    }

    fn to_containers(&self) -> Vec<Container> {
        vec![Container::from(self)]
    }
}

impl From<&OpenFaasFunctionSpec> for Probe {
    fn from(_value: &OpenFaasFunctionSpec) -> Self {
        Probe {
            http_get: Some(HTTPGetAction {
                path: Some(String::from("/_/health")),
                port: IntOrString::Int(8080),
                scheme: Some(String::from("HTTP")),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for ContainerPort {
    fn from(_value: &OpenFaasFunctionSpec) -> Self {
        ContainerPort {
            name: Some(String::from("http")),
            container_port: 8080,
            protocol: Some(String::from("TCP")),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for SecurityContext {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        SecurityContext {
            read_only_root_filesystem: value.read_only_root_filesystem,
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Container {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Container {
            name: value.to_name(),
            image: Some(value.to_image()),
            ports: Some(value.to_container_ports()),
            liveness_probe: Some(Probe::from(value)),
            readiness_probe: Some(Probe::from(value)),
            security_context: Some(SecurityContext::from(value)),
            volume_mounts: None, // TODO
            resources: None,     // TODO
            env: None,           // TODO
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for PodSpec {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        PodSpec {
            containers: value.to_containers(),
            volumes: None, // TODO
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for LabelSelector {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        LabelSelector {
            match_labels: Some(value.to_meta_labels()),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for PodTemplateSpec {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        PodTemplateSpec {
            metadata: Some(value.to_spec_template_meta()),
            spec: Some(PodSpec::from(value)),
        }
    }
}

impl From<&OpenFaasFunctionSpec> for DeploymentSpec {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector::from(value),
            template: PodTemplateSpec::from(value),
            ..Default::default()
        }
    }
}

/// Generate a fresh deployment
impl From<&OpenFaasFunctionSpec> for Deployment {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        // If Secrets are set, check if they exist

        Deployment {
            metadata: value.to_deployment_meta(),
            spec: Some(DeploymentSpec::from(value)),
            ..Default::default()
        }
    }
}

/// Generate a fresh service
impl From<&OpenFaasFunctionSpec> for Service {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        unimplemented!()
    }
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

#[derive(ThisError, Debug)]
pub enum IntoDeploymentError {
    #[error("Failed to get owner reference")]
    FailedToGetOwnerReference,
}

/// Generate a fresh deployment with refs
impl TryFrom<&OpenFaaSFunction> for Deployment {
    type Error = IntoDeploymentError;

    fn try_from(value: &OpenFaaSFunction) -> Result<Self, Self::Error> {
        let oref = value
            .controller_owner_ref(&())
            .ok_or(IntoDeploymentError::FailedToGetOwnerReference)?;

        let mut dep = Deployment::from(&value.spec);

        dep.metadata.owner_references = Some(vec![oref]);

        Ok(dep)
    }
}

#[derive(ThisError, Debug)]
pub enum IntoServiceError {
    #[error("Failed to get owner reference")]
    FailedToGetOwnerReference,
}

/// Generate a fresh service with refs
impl TryFrom<&OpenFaaSFunction> for Service {
    type Error = IntoServiceError;

    fn try_from(value: &OpenFaaSFunction) -> Result<Self, Self::Error> {
        let oref = value
            .controller_owner_ref(&())
            .ok_or(IntoServiceError::FailedToGetOwnerReference)?;

        let mut svc = Service::from(&value.spec);

        svc.metadata.owner_references = Some(vec![oref]);

        Ok(svc)
    }
}
