use super::defs::{
    DeploymentDiff, IntoDeploymentError, IntoServiceError, OpenFaaSFunction, OpenFaasFunctionSpec,
    ServiceDiff,
};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{
            Container, ContainerPort, EnvVar, HTTPGetAction, PodSpec, PodTemplateSpec, Probe,
            ResourceRequirements, SecurityContext, Service, ServicePort, ServiceSpec, Volume,
            VolumeMount,
        },
    },
    apimachinery::pkg::{
        api::resource::Quantity, apis::meta::v1::LabelSelector, util::intstr::IntOrString,
    },
};
use kube::core::{CustomResourceExt, ObjectMeta, Resource};
use std::collections::BTreeMap;

impl OpenFaasFunctionSpec {
    pub fn deployment_diffs(&self, deployment: &Deployment) -> Vec<DeploymentDiff> {
        unimplemented!()
    }

    pub fn service_diffs(&self, service: &Service) -> Vec<ServiceDiff> {
        unimplemented!()
    }

    fn should_create_tmp_volume(&self) -> bool {
        self.read_only_root_filesystem.unwrap_or(false)
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

    fn to_service_selector_labels(&self) -> BTreeMap<String, String> {
        self.to_meta_labels()
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

    fn to_limits(&self) -> Option<BTreeMap<String, Quantity>> {
        self.limits.clone().map(|r| {
            let mut limits = BTreeMap::new();

            if let Some(cpu) = r.cpu {
                limits.insert(String::from("cpu"), Quantity(cpu));
            }
            if let Some(memory) = r.memory {
                limits.insert(String::from("memory"), Quantity(memory));
            }
            limits
        })
    }

    fn to_requests(&self) -> Option<BTreeMap<String, Quantity>> {
        self.requests.clone().map(|r| {
            let mut requests = BTreeMap::new();
            if let Some(cpu) = r.cpu {
                requests.insert(String::from("cpu"), Quantity(cpu));
            }
            if let Some(memory) = r.memory {
                requests.insert(String::from("memory"), Quantity(memory));
            }
            requests
        })
    }

    fn to_tmp_volume(&self) -> Volume {
        Volume {
            name: String::from("tmp"),
            empty_dir: Some(Default::default()),
            ..Default::default()
        }
    }

    fn to_tmp_volume_mount(&self) -> VolumeMount {
        VolumeMount {
            name: String::from("tmp"),
            mount_path: String::from("/tmp"),
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

impl From<&OpenFaasFunctionSpec> for Option<Vec<EnvVar>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        if let Some(env_vars) = value.env_vars.clone() {
            let env_vars: Vec<EnvVar> = env_vars
                .into_iter()
                .map(|(k, v)| EnvVar {
                    name: k,
                    value: Some(v),
                    ..Default::default()
                })
                .collect();

            Some(env_vars)
        } else {
            None
        }
    }
}

impl From<&OpenFaasFunctionSpec> for ResourceRequirements {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        ResourceRequirements {
            limits: value.to_limits(),
            requests: value.to_requests(),
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Option<ResourceRequirements> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(ResourceRequirements::from(value))
    }
}

impl From<&OpenFaasFunctionSpec> for Container {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Container {
            name: value.to_name(),
            image: Some(value.to_image()),
            ports: Option::<Vec<ContainerPort>>::from(value),
            liveness_probe: Option::<Probe>::from(value),
            readiness_probe: Option::<Probe>::from(value),
            security_context: Option::<SecurityContext>::from(value),
            volume_mounts: Option::<Vec<VolumeMount>>::from(value),
            resources: Option::<ResourceRequirements>::from(value),
            env: Option::<Vec<EnvVar>>::from(value),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<VolumeMount> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let mut volume_mounts = Vec::new();

        if value.should_create_tmp_volume() {
            volume_mounts.push(value.to_tmp_volume_mount());
        }

        volume_mounts
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<VolumeMount>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let volume_mounts = Vec::<VolumeMount>::from(value);

        if volume_mounts.is_empty() {
            None
        } else {
            Some(volume_mounts)
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<Container> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        vec![Container::from(value)]
    }
}

impl From<&OpenFaasFunctionSpec> for Vec<Volume> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let mut volumes = Vec::new();

        if value.should_create_tmp_volume() {
            volumes.push(value.to_tmp_volume());
        }

        volumes
    }
}

impl From<&OpenFaasFunctionSpec> for Option<Vec<Volume>> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        let volumes = Vec::<Volume>::from(value);

        if volumes.is_empty() {
            None
        } else {
            Some(volumes)
        }
    }
}

impl From<&OpenFaasFunctionSpec> for PodSpec {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        PodSpec {
            containers: Vec::<Container>::from(value),
            volumes: Option::<Vec<Volume>>::from(value),
            ..Default::default()
        }
    }
}

impl From<&OpenFaasFunctionSpec> for Option<PodSpec> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(PodSpec::from(value))
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
            spec: Option::<PodSpec>::from(value),
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

impl From<&OpenFaasFunctionSpec> for Option<DeploymentSpec> {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        Some(DeploymentSpec::from(value))
    }
}

/// Generate a fresh deployment
impl From<&OpenFaasFunctionSpec> for Deployment {
    fn from(value: &OpenFaasFunctionSpec) -> Self {
        // If Secrets are set, check if they exist

        Deployment {
            metadata: value.to_deployment_meta(),
            spec: Option::<DeploymentSpec>::from(value),
            ..Default::default()
        }
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
            .ok_or(IntoDeploymentError::FailedToGetOwnerReference)?;

        let mut dep = Deployment::from(&value.spec);

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
            .ok_or(IntoServiceError::FailedToGetOwnerReference)?;

        let mut svc = Service::from(&value.spec);

        svc.metadata.owner_references = Some(vec![oref]);

        Ok(svc)
    }
}
