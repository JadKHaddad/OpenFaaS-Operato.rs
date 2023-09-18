use super::Operator;
use crate::consts::{DEFAULT_IMAGE_WITH_TAG, PKG_NAME, PKG_VERSION};
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

//TODO: normal function with param for namespace and image

const NAME: &str = "openfaas-functions-operator";

impl Operator {
    fn to_labels(&self) -> BTreeMap<String, String> {
        [(String::from("app"), NAME.to_string())].into()
    }
}

impl From<&Operator> for ObjectMeta {
    fn from(value: &Operator) -> Self {
        let namespace = value.functions_namespace();
        ObjectMeta {
            name: Some(NAME.to_string()),
            namespace: Some(namespace.into()),
            ..Default::default()
        }
    }
}

impl From<&Operator> for ServiceAccount {
    fn from(value: &Operator) -> Self {
        ServiceAccount {
            metadata: ObjectMeta::from(value),
            ..Default::default()
        }
    }
}

impl From<&Operator> for Role {
    fn from(value: &Operator) -> Self {
        let namespace = value.functions_namespace();
        Role {
            metadata: ObjectMeta {
                name: Some(NAME.to_string()),
                namespace: Some(namespace.into()),
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

impl From<&Operator> for RoleBinding {
    fn from(value: &Operator) -> Self {
        RoleBinding {
            metadata: ObjectMeta::from(value),
            subjects: Some(vec![Subject {
                kind: String::from("ServiceAccount"),
                name: NAME.to_string(),
                namespace: Some(value.functions_namespace().into()),
                ..Default::default()
            }]),
            role_ref: RoleRef {
                kind: String::from("Role"),
                name: String::from(NAME),
                api_group: String::from("rbac.authorization.k8s.io"),
            },
        }
    }
}

impl From<&Operator> for Deployment {
    fn from(value: &Operator) -> Self {
        Deployment {
            metadata: ObjectMeta::from(value),
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
                        service_account_name: Some(NAME.to_string()),
                        containers: vec![Container {
                            name: NAME.to_string(),
                            image: Some(format!("{}:{}", DEFAULT_IMAGE_WITH_TAG, PKG_VERSION)),
                            args: Some(vec![String::from("run"), String::from("controller")]),
                            env: Some(vec![EnvVar {
                                name: String::from("RUST_LOG"),
                                value: Some(format!("{PKG_NAME}=info")),
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
