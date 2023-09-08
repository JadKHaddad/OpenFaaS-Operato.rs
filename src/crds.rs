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
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;

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
}

/// Generate a fresh deployment with refs
impl From<&OpenFaasFunctionSpec> for Deployment {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        // If Secrets are set, check if they exist

        let mut dep_meta_labels = BTreeMap::new();
        dep_meta_labels.insert(String::from("faas_function"), value.service.clone());

        let annotations = if let Some(annotations) = value.annotations.clone() {
            let annotations: BTreeMap<String, String> = annotations.into_iter().collect();
            Some(annotations)
        } else {
            None
        };

        let dep_metadata = ObjectMeta {
            name: Some(value.service.clone()),
            namespace: value.namespace.clone(),
            labels: Some(dep_meta_labels.clone()),
            annotations: annotations.clone(),
            ..Default::default()
        };

        let mut spec_template_metadata_labels = dep_meta_labels.clone();
        if let Some(lables) = value.labels.clone() {
            let lables: BTreeMap<String, String> = lables.into_iter().collect();
            spec_template_metadata_labels.extend(lables);
        }

        let spec_template_metadata = ObjectMeta {
            name: Some(value.service.clone()),
            labels: Some(spec_template_metadata_labels),
            annotations,
            ..Default::default()
        };

        let ports = vec![ContainerPort {
            name: Some(String::from("http")),
            container_port: 8080,
            protocol: Some(String::from("TCP")),
            ..Default::default()
        }];

        let readiness_and_liveness_probe = Probe {
            http_get: Some(HTTPGetAction {
                path: Some(String::from("/_/health")),
                port: IntOrString::Int(8080),
                scheme: Some(String::from("HTTP")),
                ..Default::default()
            }),
            ..Default::default()
        };

        let security_context = SecurityContext {
            read_only_root_filesystem: value.read_only_root_filesystem,
            ..Default::default()
        };

        let containers = vec![Container {
            name: value.service.clone(),
            image: Some(value.image.clone()),
            ports: Some(ports),
            liveness_probe: Some(readiness_and_liveness_probe.clone()),
            readiness_probe: Some(readiness_and_liveness_probe),
            security_context: Some(security_context),
            volume_mounts: None, // TODO
            resources: None,     // TODO
            env: None,           // TODO
            ..Default::default()
        }];

        let spec_template_spec = PodSpec {
            containers,
            volumes: None, // TODO
            ..Default::default()
        };

        let spec = DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector {
                match_labels: Some(dep_meta_labels),
                ..Default::default()
            },
            template: PodTemplateSpec {
                metadata: Some(spec_template_metadata),
                spec: Some(spec_template_spec),
            },
            ..Default::default()
        };

        Deployment {
            metadata: dep_metadata,
            spec: Some(spec),
            ..Default::default()
        }
    }
}

/// Generate a fresh service with refs
impl From<&OpenFaasFunctionSpec> for Service {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        unimplemented!()
    }
}
