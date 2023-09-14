use crate::utils;

use super::defs::{
    DeploymentDiff, FunctionResources, FunctionResourcesQuantity, IntoDeploymentError,
    IntoQuantityError, IntoServiceError, OpenFaaSFunction, OpenFaasFunctionErrorStatus,
    OpenFaasFunctionOkStatus, OpenFaasFunctionSpec, ServiceDiff,
};
use itertools::Itertools;
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec, DeploymentStrategy, RollingUpdateDeployment},
        core::v1::{
            Container, ContainerPort, EnvVar, HTTPGetAction, KeyToPath, PodSpec, PodTemplateSpec,
            Probe, ProjectedVolumeSource, ResourceRequirements, SecretProjection, SecurityContext,
            Service, ServicePort, ServiceSpec, Volume, VolumeMount, VolumeProjection,
        },
    },
    apimachinery::pkg::{
        api::resource::Quantity, apis::meta::v1::LabelSelector, util::intstr::IntOrString,
    },
};
use kube::{
    api::Patch,
    core::{CustomResourceExt, ObjectMeta, Resource},
};
use kube_quantity::ParsedQuantity;
use std::{collections::BTreeMap, fmt::Display};

impl Display for OpenFaasFunctionOkStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenFaasFunctionOkStatus::Ready => {
                write!(f, "Deployment is ready")
            }
        }
    }
}

impl FunctionResources {
    fn try_to_k8s_resources(
        &self,
    ) -> Result<Option<BTreeMap<String, Quantity>>, IntoQuantityError> {
        Ok(FunctionResourcesQuantity::try_from(self)?.to_k8s_resources())
    }
}

impl FunctionResourcesQuantity {
    fn to_k8s_resources(&self) -> Option<BTreeMap<String, Quantity>> {
        let mut resources = BTreeMap::new();

        if let Some(cpu) = self.cpu.clone() {
            resources.insert(String::from("cpu"), cpu);
        }

        if let Some(memory) = self.memory.clone() {
            resources.insert(String::from("memory"), memory);
        }

        if resources.is_empty() {
            return None;
        }

        Some(resources)
    }
}

impl TryFrom<&FunctionResources> for FunctionResourcesQuantity {
    type Error = IntoQuantityError;

    fn try_from(value: &FunctionResources) -> Result<Self, Self::Error> {
        let memory: Option<Quantity> = value
            .memory
            .clone()
            .map(|m| ParsedQuantity::try_from(m).map_err(IntoQuantityError::Memory))
            .transpose()?
            .map(|m| m.into());

        let cpu: Option<Quantity> = value
            .cpu
            .clone()
            .map(|m| ParsedQuantity::try_from(m).map_err(IntoQuantityError::CPU))
            .transpose()?
            .map(|m| m.into());

        Ok(Self { memory, cpu })
    }
}

impl OpenFaasFunctionSpec {
    pub fn deployment_needs_recreation(&self, deployment: &Deployment) -> bool {
        // service, and namespace as metadata fields, cannot be changed in deployment
        // service if changed in crd, will trigger a creation of a new deployment and old one will be deleted
        // namepace is logically impossible to change, as it is a crd namespace and the deployment namespace
        // changing namespace will cause a stuck status, as old deployments will not be deleted, and the status will be set to invalidfunctionNamespce
        // compare image,
        // container,
        // envs, env_process, secrets, limits, requests,
        // constraints, read_only_root_filesystem,
        // secrets_mount_path
        unimplemented!()
    }

    pub fn deployment_diffs(&self, deployment: &Deployment) -> Vec<DeploymentDiff> {
        unimplemented!()
    }

    pub fn service_diffs(&self, service: &Service) -> Vec<ServiceDiff> {
        unimplemented!()
    }

    fn should_create_tmp_volume(&self) -> bool {
        self.read_only_root_filesystem.unwrap_or(false)
    }

    fn should_create_secrets_volume(&self) -> bool {
        !self.secrets.as_ref().unwrap_or(&vec![]).is_empty()
    }

    pub fn get_secrets_unique_vec(&self) -> Vec<String> {
        self.secrets
            .clone()
            .unwrap_or(vec![])
            .into_iter()
            .unique()
            .collect()
    }

    pub fn get_constraints_vec(&self) -> Vec<String> {
        self.constraints.clone().unwrap_or(vec![])
    }

    fn to_env_process_name(&self) -> String {
        String::from("fprocess")
    }

    pub fn to_name(&self) -> String {
        self.service.clone()
    }

    fn to_namespace(&self) -> Option<String> {
        self.namespace.clone()
    }

    fn to_image(&self) -> String {
        self.image.clone()
    }

    fn to_meta_labels(&self) -> BTreeMap<String, String> {
        [(String::from("faas_function"), self.to_name())].into()
    }

    fn to_spec_meta_labels(&self) -> BTreeMap<String, String> {
        self.labels
            .clone()
            .map(|lables| {
                let mut labels: BTreeMap<String, String> = lables.into_iter().collect();

                labels.extend(self.to_meta_labels());

                labels
            })
            .unwrap_or(self.to_meta_labels())
    }

    fn to_service_selector_labels(&self) -> BTreeMap<String, String> {
        self.to_meta_labels()
    }

    fn to_annotations(&self) -> Option<BTreeMap<String, String>> {
        self.annotations.clone().map(|a| a.into_iter().collect())
    }

    fn to_node_selector(&self) -> Option<BTreeMap<String, String>> {
        let constraints = self.get_constraints_vec();

        if constraints.is_empty() {
            return None;
        }

        let node_selector: BTreeMap<String, String> = constraints
            .iter()
            .map(|c| c.split("==").collect::<Vec<&str>>())
            .filter(|v| v.len() == 2)
            .map(|v| {
                (
                    utils::remove_whitespace(v[0]),
                    utils::remove_whitespace(v[1]),
                )
            })
            .unique()
            .collect();

        Some(node_selector)
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

    fn to_service_meta(&self) -> ObjectMeta {
        self.to_deployment_meta()
    }

    fn to_spec_template_meta(&self) -> ObjectMeta {
        ObjectMeta {
            name: Some(self.to_name()),
            labels: Some(self.to_spec_meta_labels()),
            annotations: self.to_annotations(),
            ..Default::default()
        }
    }

    fn try_to_limits(&self) -> Result<Option<BTreeMap<String, Quantity>>, IntoQuantityError> {
        if let Some(ref limits) = self.limits {
            return limits.try_to_k8s_resources();
        }

        Ok(None)
    }

    fn try_to_requests(&self) -> Result<Option<BTreeMap<String, Quantity>>, IntoQuantityError> {
        if let Some(ref requests) = self.requests {
            return requests.try_to_k8s_resources();
        }

        Ok(None)
    }

    fn to_tmp_volume_name(&self) -> String {
        String::from("tmp")
    }

    fn to_tmp_volume(&self) -> Volume {
        Volume {
            name: self.to_tmp_volume_name(),
            empty_dir: Some(Default::default()),
            ..Default::default()
        }
    }

    fn to_tmp_volume_mount_path(&self) -> String {
        String::from("/tmp")
    }

    fn to_tmp_volume_mount(&self) -> VolumeMount {
        VolumeMount {
            name: self.to_tmp_volume_name(),
            mount_path: self.to_tmp_volume_mount_path(),
            ..Default::default()
        }
    }

    fn to_secrets_volume_name(&self) -> String {
        format!("{}-projected-secrets", self.to_name())
    }

    fn to_secrets_projected_volume_source(&self) -> Option<ProjectedVolumeSource> {
        let secrets = self.get_secrets_unique_vec();

        if secrets.is_empty() {
            return None;
        }

        let sources = secrets
            .iter()
            .map(|secret| {
                let items = vec![KeyToPath {
                    key: secret.clone(),
                    path: secret.clone(),
                    ..Default::default()
                }];

                VolumeProjection {
                    secret: Some(SecretProjection {
                        name: Some(secret.clone()),
                        items: Some(items),
                        ..Default::default()
                    }),
                    ..Default::default()
                }
            })
            .collect();

        Some(ProjectedVolumeSource {
            sources: Some(sources),
            ..Default::default()
        })
    }

    fn to_secrets_volume(&self) -> Volume {
        Volume {
            name: self.to_secrets_volume_name(),
            projected: self.to_secrets_projected_volume_source(),
            ..Default::default()
        }
    }

    fn to_default_secrets_mount_path(&self) -> String {
        String::from("/var/openfaas/secrets")
    }

    fn to_secrets_mount_path(&self) -> String {
        self.secrets_mount_path
            .clone()
            .unwrap_or(self.to_default_secrets_mount_path())
    }

    fn to_secrets_volume_mount(&self) -> VolumeMount {
        VolumeMount {
            name: self.to_secrets_volume_name(),
            mount_path: self.to_secrets_mount_path(),
            read_only: Some(true),
            ..Default::default()
        }
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

impl From<&OpenFaasFunctionSpec> for Option<Probe> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(Probe::from(value))
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

impl From<&OpenFaasFunctionSpec> for Vec<ContainerPort> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        vec![ContainerPort::from(value)]
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<ContainerPort>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(Vec::<ContainerPort>::from(value))
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

impl From<&OpenFaasFunctionSpec> for Option<SecurityContext> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(SecurityContext::from(value))
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<EnvVar> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let mut env_vars = Vec::new();

        if let Some(env_process) = value.env_process.clone() {
            env_vars.push(EnvVar {
                name: value.to_env_process_name(),
                value: Some(env_process),
                ..Default::default()
            });
        }

        if let Some(env_vars_map) = value.env_vars.clone() {
            for (k, v) in env_vars_map {
                env_vars.push(EnvVar {
                    name: k,
                    value: Some(v),
                    ..Default::default()
                });
            }
        }

        env_vars
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<EnvVar>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let env_vars = Vec::<EnvVar>::from(value);

        if env_vars.is_empty() {
            return None;
        }

        Some(env_vars)
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for ResourceRequirements {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(ResourceRequirements {
            limits: value.try_to_limits()?,
            requests: value.try_to_requests()?,
        })
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for Option<ResourceRequirements> {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(Some(ResourceRequirements::try_from(value)?))
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for Container {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(Container {
            name: value.to_name(),
            image: Some(value.to_image()),
            ports: Option::<Vec<ContainerPort>>::from(value),
            liveness_probe: Option::<Probe>::from(value),
            readiness_probe: Option::<Probe>::from(value),
            security_context: Option::<SecurityContext>::from(value),
            volume_mounts: Option::<Vec<VolumeMount>>::from(value),
            resources: Option::<ResourceRequirements>::try_from(value)?,
            env: Option::<Vec<EnvVar>>::from(value),
            ..Default::default()
        })
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<VolumeMount> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let mut volume_mounts = Vec::new();

        if value.should_create_tmp_volume() {
            volume_mounts.push(value.to_tmp_volume_mount());
        }

        if value.should_create_secrets_volume() {
            volume_mounts.push(value.to_secrets_volume_mount());
        }

        volume_mounts
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<VolumeMount>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let volume_mounts = Vec::<VolumeMount>::from(value);

        if volume_mounts.is_empty() {
            return None;
        }

        Some(volume_mounts)
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for Vec<Container> {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(vec![Container::try_from(value)?])
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<Volume> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let mut volumes = Vec::new();

        if value.should_create_tmp_volume() {
            volumes.push(value.to_tmp_volume());
        }

        if value.should_create_secrets_volume() {
            volumes.push(value.to_secrets_volume());
        }

        volumes
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<Volume>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let volumes = Vec::<Volume>::from(value);

        if volumes.is_empty() {
            return None;
        }

        Some(volumes)
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for PodSpec {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(PodSpec {
            containers: Vec::<Container>::try_from(value)?,
            volumes: Option::<Vec<Volume>>::from(value),
            node_selector: value.to_node_selector(),
            ..Default::default()
        })
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for Option<PodSpec> {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(Some(PodSpec::try_from(value)?))
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

impl From<&OpenFaasFunctionSpec> for RollingUpdateDeployment {
    fn from(_value: &OpenFaasFunctionSpec) -> Self {
        RollingUpdateDeployment {
            max_surge: Some(IntOrString::Int(1)),
            max_unavailable: Some(IntOrString::Int(0)),
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Option<RollingUpdateDeployment> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(RollingUpdateDeployment::from(value))
    }
}

impl From<&OpenFaasFunctionSpec> for DeploymentStrategy {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        DeploymentStrategy {
            rolling_update: Option::<RollingUpdateDeployment>::from(value),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Option<DeploymentStrategy> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(DeploymentStrategy::from(value))
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for PodTemplateSpec {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(PodTemplateSpec {
            metadata: Some(value.to_spec_template_meta()),
            spec: Option::<PodSpec>::try_from(value)?,
        })
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for DeploymentSpec {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(DeploymentSpec {
            replicas: Some(1),
            selector: LabelSelector::from(value),
            strategy: Option::<DeploymentStrategy>::from(value),
            template: PodTemplateSpec::try_from(value)?,
            ..Default::default()
        })
    }
}

impl TryFrom<&OpenFaasFunctionSpec> for Option<DeploymentSpec> {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(Some(DeploymentSpec::try_from(value)?))
    }
}

/// Generate a fresh deployment
impl TryFrom<&OpenFaasFunctionSpec> for Deployment {
    type Error = IntoQuantityError;

    fn try_from(value: &OpenFaasFunctionSpec) -> Result<Self, Self::Error> {
        Ok(Deployment {
            metadata: value.to_deployment_meta(),
            spec: Option::<DeploymentSpec>::try_from(value)?,
            ..Default::default()
        })
    }
}

impl From<&OpenFaasFunctionSpec> for ServicePort {
    fn from(_value: &OpenFaasFunctionSpec) -> Self {
        ServicePort {
            name: Some(String::from("http")),
            port: 8080,
            target_port: Some(IntOrString::Int(8080)),
            protocol: Some(String::from("TCP")),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<ServicePort> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        vec![ServicePort::from(value)]
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<ServicePort>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(Vec::<ServicePort>::from(value))
    }
}

impl From<&OpenFaasFunctionSpec> for ServiceSpec {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        ServiceSpec {
            selector: Some(value.to_service_selector_labels()),
            ports: Option::<Vec<ServicePort>>::from(value),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Option<ServiceSpec> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(ServiceSpec::from(value))
    }
}

/// Generate a fresh service
impl From<&OpenFaasFunctionSpec> for Service {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Service {
            metadata: value.to_service_meta(),
            spec: Option::<ServiceSpec>::from(value),
            ..Default::default()
        }
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

/// Generate a fresh deployment with refs
impl TryFrom<&OpenFaaSFunction> for Deployment {
    type Error = IntoDeploymentError;

    fn try_from(value: &OpenFaaSFunction) -> Result<Self, Self::Error> {
        let oref = value
            .controller_owner_ref(&())
            .ok_or(IntoDeploymentError::OwnerReference)?;

        let mut dep = Deployment::try_from(&value.spec).map_err(IntoDeploymentError::Quantity)?;

        dep.metadata.owner_references = Some(vec![oref]);

        Ok(dep)
    }
}

/// Generate a fresh service with refs
impl TryFrom<&OpenFaaSFunction> for Service {
    type Error = IntoServiceError;

    fn try_from(value: &OpenFaaSFunction) -> Result<Self, Self::Error> {
        let oref = value
            .controller_owner_ref(&())
            .ok_or(IntoServiceError::OwnerReference)?;

        let mut svc = Service::from(&value.spec);

        svc.metadata.owner_references = Some(vec![oref]);

        Ok(svc)
    }
}

impl From<&IntoDeploymentError> for Option<OpenFaasFunctionErrorStatus> {
    fn from(e: &IntoDeploymentError) -> Self {
        match e {
            IntoDeploymentError::OwnerReference => None,
            IntoDeploymentError::Quantity(e) => match e {
                IntoQuantityError::Memory(_) => Some(OpenFaasFunctionErrorStatus::MemoryQuantity),
                IntoQuantityError::CPU(_) => Some(OpenFaasFunctionErrorStatus::CPUQuantity),
            },
        }
    }
}
