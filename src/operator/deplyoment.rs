use super::UpdateStrategy;
use crate::consts::PKG_NAME;
use crate::crds::defs::{GROUP, PLURAL};
use k8s_openapi::{
    api::{
        apps::v1::{Deployment, DeploymentSpec},
        core::v1::{Container, EnvVar, PodSpec, PodTemplateSpec, ServiceAccount},
        rbac::v1::{PolicyRule, Role, RoleBinding, RoleRef, Subject},
    },
    apimachinery::pkg::apis::meta::v1::LabelSelector,
};
use kube::core::ObjectMeta;
use std::collections::BTreeMap;

pub struct DeploymentBuilder {
    app_name: String,
    namespace: String,
    image: String,
    update_strategy: UpdateStrategy,
}

impl DeploymentBuilder {
    pub fn new(
        app_name: String,
        namespace: String,
        image: String,
        update_strategy: UpdateStrategy,
    ) -> Self {
        Self {
            app_name,
            namespace,
            image,
            update_strategy,
        }
    }

    fn to_labels(&self) -> BTreeMap<String, String> {
        [("app".to_string(), self.app_name.clone())].into()
    }

    pub fn to_yaml_string(&self) -> Result<String, serde_yaml::Error> {
        let mut string = String::new();

        let service_account = ServiceAccount::from(self);
        let service_account_str = serde_yaml::to_string(&service_account)?;

        let role = Role::from(self);
        let role_str = serde_yaml::to_string(&role)?;

        let role_binding = RoleBinding::from(self);
        let role_binding_str = serde_yaml::to_string(&role_binding)?;

        let deployment = Deployment::from(self);
        let deployment_str = serde_yaml::to_string(&deployment)?;

        string.push_str(&service_account_str);
        string.push_str("---\n");
        string.push_str(&role_str);
        string.push_str("---\n");
        string.push_str(&role_binding_str);
        string.push_str("---\n");
        string.push_str(&deployment_str);

        Ok(string)
    }
}

impl From<&DeploymentBuilder> for ServiceAccount {
    fn from(value: &DeploymentBuilder) -> Self {
        ServiceAccount {
            metadata: ObjectMeta {
                name: Some(value.app_name.clone()),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

impl From<&DeploymentBuilder> for Role {
    fn from(value: &DeploymentBuilder) -> Self {
        Role {
            metadata: ObjectMeta {
                name: Some(format!("{}-role", value.app_name)),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            rules: Some(vec![
                PolicyRule {
                    api_groups: Some(vec![String::from(GROUP)]),
                    resources: Some(vec![
                        String::from(PLURAL),
                        format!("{}/status", PLURAL),
                        format!("{}/finalizers", PLURAL),
                    ]),
                    verbs: vec![String::from("*")],
                    ..Default::default()
                },
                PolicyRule {
                    api_groups: Some(vec![String::from("")]),
                    resources: Some(vec![String::from("namespaces")]),
                    verbs: vec![String::from("get")],
                    ..Default::default()
                },
                PolicyRule {
                    api_groups: Some(vec![String::from("")]),
                    resources: Some(vec![String::from("secrets")]),
                    verbs: vec![String::from("list")],
                    ..Default::default()
                },
                PolicyRule {
                    api_groups: Some(vec![String::from("apps")]),
                    resources: Some(vec![String::from("deployments")]),
                    verbs: vec![String::from("*")],
                    ..Default::default()
                },
                PolicyRule {
                    api_groups: Some(vec![String::from("")]),
                    resources: Some(vec![String::from("services")]),
                    verbs: vec![String::from("*")],
                    ..Default::default()
                },
            ]),
        }
    }
}

impl From<&DeploymentBuilder> for RoleBinding {
    fn from(value: &DeploymentBuilder) -> Self {
        RoleBinding {
            metadata: ObjectMeta {
                name: Some(format!("{}-rolebinding", value.app_name)),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            subjects: Some(vec![Subject {
                kind: String::from("ServiceAccount"),
                name: value.app_name.clone(),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            }]),
            role_ref: RoleRef {
                kind: String::from("Role"),
                name: format!("{}-role", value.app_name),
                api_group: String::from("rbac.authorization.k8s.io"),
            },
        }
    }
}

impl From<&DeploymentBuilder> for Deployment {
    fn from(value: &DeploymentBuilder) -> Self {
        Deployment {
            metadata: ObjectMeta {
                name: Some(value.app_name.clone()),
                namespace: Some(value.namespace.clone()),
                ..Default::default()
            },
            spec: Some(DeploymentSpec {
                replicas: Some(1),
                selector: LabelSelector {
                    match_labels: Some(value.to_labels()),
                    ..Default::default()
                },
                template: PodTemplateSpec {
                    metadata: Some(ObjectMeta {
                        labels: Some(value.to_labels()),
                        ..Default::default()
                    }),
                    spec: Some(PodSpec {
                        service_account_name: Some(value.app_name.clone()),
                        containers: vec![Container {
                            name: value.app_name.clone(),
                            image: Some(value.image.clone()),
                            args: Some(vec![
                                // TODO: use consts
                                String::from("operator"),
                                String::from("controller"),
                                String::from("--functions-namespace"),
                                value.namespace.clone(),
                                String::from("--update-strategy"),
                                value.update_strategy.to_string(),
                                String::from("run"),
                            ]),
                            env: Some(vec![EnvVar {
                                name: String::from("RUST_LOG"),
                                value: Some(format!("{PKG_NAME}=info,kube=off")),
                                ..Default::default()
                            }]),
                            ..Default::default()
                        }],
                        ..Default::default()
                    }),
                },
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
