use crate::controller::errors::*;
use crate::crds::defs::{
    OpenFaaSFunction, OpenFaasFunctionErrorStatus, OpenFaasFunctionOkStatus,
    OpenFaasFunctionStatus, FINALIZER_NAME,
};
use futures::stream::StreamExt;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Secret, Service},
};
use kube::{
    api::{ListParams, PostParams},
    runtime::{controller::Action, finalizer::Event, watcher::Config},
    runtime::{finalizer, Controller},
    Api, Client as KubeClient, Resource, ResourceExt,
};
use std::sync::Arc;
use tokio::time::Duration;
use tracing::{trace_span, Instrument};

struct OperatorInner {
    kubernetes_client: KubeClient,
    functions_namespace: String,
    api: Api<OpenFaaSFunction>,
    deployment_api: Api<Deployment>,
    service_api: Api<Service>,
    secrets_api: Api<Secret>,
}

impl OperatorInner {
    fn new(kubernetes_client: KubeClient, functions_namespace: String) -> Self {
        let api: Api<OpenFaaSFunction> =
            Api::namespaced(kubernetes_client.clone(), &functions_namespace);
        let deployment_api: Api<Deployment> =
            Api::namespaced(kubernetes_client.clone(), &functions_namespace);
        let service_api: Api<Service> =
            Api::namespaced(kubernetes_client.clone(), &functions_namespace);
        let secrets_api: Api<Secret> =
            Api::namespaced(kubernetes_client.clone(), &functions_namespace);

        Self {
            kubernetes_client,
            functions_namespace,
            api,
            deployment_api,
            service_api,
            secrets_api,
        }
    }

    async fn reconcile(&self, crd: Arc<OpenFaaSFunction>) -> Result<Action, ReconcileError> {
        let name = crd.name_any();

        let Some(crd_namespace) = crd.namespace() else {
            tracing::error!(%name, "Resource has no namespace. Aborting.");
            return Err(ReconcileError::Namespace);
        };

        let reconcile_resource_span = trace_span!("ReconcileResource", %name, %crd_namespace);

        let api = self.api.clone();

        finalizer(&api, FINALIZER_NAME, crd, |event| async move {
            match event {
                Event::Apply(crd) => {
                    let apply_resource_span = trace_span!("ApplyResource");

                    self.apply(crd, &crd_namespace)
                        .instrument(apply_resource_span)
                        .await
                        .map_err(FinalizeError::Apply)
                }
                Event::Cleanup(crd) => {
                    let cleanup_resource_span = trace_span!("CleanupResource");

                    self.cleanup(crd, &crd_namespace)
                        .instrument(cleanup_resource_span)
                        .await
                        .map_err(FinalizeError::Cleanup)
                }
            }
        })
        .instrument(reconcile_resource_span)
        .await
        .map_err(ReconcileError::FinalizeError)
    }

    async fn apply(
        &self,
        crd: Arc<OpenFaaSFunction>,
        crd_namespace: &str,
    ) -> Result<Action, ApplyError> {
        tracing::info!("Applying resource.");

        let functions_namespace = &self.functions_namespace;

        let check_res_namespace_span = trace_span!("CheckResourceNamespace", %functions_namespace);
        if let Some(action) = self
            .check_resource_namespace(&crd, crd_namespace)
            .instrument(check_res_namespace_span)
            .await
            .map_err(ApplyError::ResourceNamespace)?
        {
            return Ok(action);
        }

        let check_fun_namespace_span = trace_span!("CheckFunctionNamespace", %functions_namespace);
        if let Some(action) = self
            .check_function_namespace(&crd)
            .instrument(check_fun_namespace_span)
            .await
            .map_err(ApplyError::FunctionNamespace)?
        {
            return Ok(action);
        }

        let check_deployment_span = trace_span!("CheckDeployment");
        if let Some(action) = self
            .check_deployment(&crd)
            .instrument(check_deployment_span)
            .await
            .map_err(ApplyError::Deployment)?
        {
            return Ok(action);
        }

        let check_service_span = trace_span!("CheckService");
        if let Some(action) = self
            .check_service(&crd)
            .instrument(check_service_span)
            .await
            .map_err(ApplyError::Service)?
        {
            return Ok(action);
        }

        let set_status_span = trace_span!("SetDeployedStatus");
        if let Some(action) = self
            .set_deployed_status(&crd)
            .instrument(set_status_span)
            .await
            .map_err(ApplyError::Status)?
        {
            return Ok(action);
        }

        tracing::info!("Awaiting change.");

        Ok(Action::await_change())
    }

    async fn cleanup(
        &self,
        _crd: Arc<OpenFaaSFunction>,
        _crd_namespace: &str,
    ) -> Result<Action, CleanupError> {
        tracing::info!("Cleaning up resource.");

        tracing::info!("Nothing to do here. We use OwnerReferences.");

        tracing::info!("Awaiting change.");

        Ok(Action::await_change())
    }

    async fn replace_status(
        &self,
        crd_with_status: &mut OpenFaaSFunction,
        status: OpenFaasFunctionStatus,
    ) -> Result<(), StatusError> {
        let name = crd_with_status.name_any();
        let api = &self.api;

        match crd_with_status.status {
            Some(ref func_status) if func_status == &status => {
                tracing::info!("Resource already has {:?} status. Skipping.", status);
            }
            _ => {
                tracing::info!("Setting status to {:?}.", status);

                crd_with_status.status = Some(status.clone());
                api.replace_status(
                    &name,
                    &PostParams::default(),
                    serde_json::to_vec(&crd_with_status).map_err(|error| StatusError {
                        error: SetStatusError::Serilization(error),
                        status: status.clone(),
                    })?,
                )
                .await
                .map_err(|error| StatusError {
                    error: SetStatusError::Kube(error),
                    status: status.clone(),
                })?;

                tracing::info!("Status set to {:?}.", status);
            }
        }

        Ok(())
    }

    async fn check_resource_namespace(
        &self,
        crd: &OpenFaaSFunction,
        crd_namespace: &str,
    ) -> Result<Option<Action>, CheckResourceNamespaceError> {
        tracing::info!("Comparing resource's namespace to functions namespace.");

        let name = crd.name_any();
        let functions_namespace = &self.functions_namespace;
        let api = &self.api;

        if crd_namespace != functions_namespace {
            tracing::error!("Resource's namespace does not match functions namespace.");

            let mut crd_with_status = api
                .get_status(&name)
                .await
                .map_err(CheckResourceNamespaceError::Kube)?;

            let status =
                OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::InvalidCRDNamespace);

            self.replace_status(&mut crd_with_status, status)
                .await
                .map_err(CheckResourceNamespaceError::Status)?;

            tracing::info!("Requeueing resource.");

            return Ok(Some(Action::requeue(Duration::from_secs(10))));
        }

        Ok(None)
    }

    async fn check_function_namespace(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, CheckFunctionNamespaceError> {
        tracing::info!("Comparing functions's namespace to functions namespace.");

        let name = crd.name_any();
        let functions_namespace = &self.functions_namespace;
        let api = &self.api;

        match crd.spec.namespace {
            None => {
                tracing::info!(default = %functions_namespace, "Function has no namespace. Assuming default.");
            }

            Some(ref function_namespace) => {
                tracing::info!(%function_namespace, "Function has namespace.");
                tracing::info!(%function_namespace, "Comparing function's namespace to functions namespace.");

                if function_namespace != functions_namespace {
                    tracing::error!(%function_namespace, "Function's namespace does not match functions namespace.");

                    let mut crd_with_status = api
                        .get_status(&name)
                        .await
                        .map_err(CheckFunctionNamespaceError::Kube)?;

                    let status = OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::InvalidFunctionNamespace,
                    );

                    self.replace_status(&mut crd_with_status, status)
                        .await
                        .map_err(CheckFunctionNamespaceError::Status)?;

                    tracing::info!("Requeueing resource.");

                    return Ok(Some(Action::requeue(Duration::from_secs(10))));
                }
            }
        }

        Ok(None)
    }

    async fn check_deployment(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, DeploymentError> {
        tracing::info!("Checking if deployment exists.");

        let name = crd.name_any();
        let api = &self.api;
        let deployment_api = &self.deployment_api;

        let deployment_opt = deployment_api
            .get_opt(&name)
            .await
            .map_err(DeploymentError::Get)?;

        match deployment_opt {
            Some(deployment) => {
                tracing::info!("Deployment exists. Comparing.");

                let crd_oref = crd
                    .controller_owner_ref(&())
                    .ok_or(DeploymentError::OwnerReference)?;

                let deployment_orefs = deployment.owner_references();

                if !deployment_orefs.contains(&crd_oref) {
                    tracing::error!("Deployment does not have owner reference.");

                    let mut crd_with_status =
                        api.get_status(&name).await.map_err(DeploymentError::Kube)?;

                    let status = OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::DeploymentAlreadyExists,
                    );

                    self.replace_status(&mut crd_with_status, status)
                        .await
                        .map_err(DeploymentError::Status)?;

                    tracing::info!("Requeueing resource.");

                    return Ok(Some(Action::requeue(Duration::from_secs(10))));
                }

                // TODO: Compare deployment
            }
            None => {
                tracing::info!("Deployment does not exist. Creating.");

                let check_secrets_span = trace_span!("CheckSecrets");
                if let Some(action) = self
                    .check_secrets(crd)
                    .instrument(check_secrets_span)
                    .await
                    .map_err(DeploymentError::Secrets)?
                {
                    return Ok(Some(action));
                }

                let deployment = Deployment::try_from(crd).map_err(DeploymentError::Generate)?;
                deployment_api
                    .create(&PostParams::default(), &deployment)
                    .await
                    .map_err(DeploymentError::Apply)?;

                tracing::info!("Deployment created.");
            }
        }

        Ok(None)
    }

    async fn check_secrets(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, CheckSecretsError> {
        tracing::info!("Checking if secrets exist.");

        let secrets = crd.spec.get_secrets_unique_vec();
        if !secrets.is_empty() {
            let name = crd.name_any();
            let api = &self.api;
            let secrets_api = &self.secrets_api;

            let existing_secret_names: Vec<String> = secrets_api
                .list(&ListParams::default())
                .await
                .map_err(CheckSecretsError::Kube)?
                .into_iter()
                .map(|secret| secret.metadata.name.unwrap_or_default())
                .collect();

            let not_found_secret_names: Vec<String> = secrets
                .iter()
                .filter(|secret| !existing_secret_names.contains(secret))
                .cloned()
                .collect();

            if !not_found_secret_names.is_empty() {
                let not_found_secret_names_str = not_found_secret_names.join(", ");
                tracing::error!("Secret(s) {} do(es) not exist.", not_found_secret_names_str);

                let mut crd_with_status = api
                    .get_status(&name)
                    .await
                    .map_err(CheckSecretsError::Kube)?;

                let status =
                    OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::SecretsNotFound);

                self.replace_status(&mut crd_with_status, status)
                    .await
                    .map_err(CheckSecretsError::Status)?;

                tracing::info!("Requeueing resource.");

                return Ok(Some(Action::requeue(Duration::from_secs(10))));
            }
        }

        tracing::info!("Secrets exist.");

        Ok(None)
    }

    async fn check_service(&self, crd: &OpenFaaSFunction) -> Result<Option<Action>, ServiceError> {
        tracing::info!("Checking if service exists.");

        let name = crd.name_any();
        let api = &self.api;
        let service_api = &self.service_api;

        let service_opt = service_api
            .get_opt(&name)
            .await
            .map_err(ServiceError::Get)?;

        match service_opt {
            Some(service) => {
                tracing::info!("Service exists. Comparing.");

                let crd_oref = crd
                    .controller_owner_ref(&())
                    .ok_or(ServiceError::OwnerReference)?;

                let service_orefs = service.owner_references();

                if !service_orefs.contains(&crd_oref) {
                    tracing::error!("Service does not have owner reference.");

                    let mut crd_with_status =
                        api.get_status(&name).await.map_err(ServiceError::Kube)?;

                    let status = OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::ServiceAlreadyExists,
                    );

                    self.replace_status(&mut crd_with_status, status)
                        .await
                        .map_err(ServiceError::Status)?;

                    tracing::info!("Requeueing resource.");

                    return Ok(Some(Action::requeue(Duration::from_secs(10))));
                }

                // TODO: Compare service
            }
            None => {
                tracing::info!("Service does not exist. Creating.");
                let service = Service::try_from(crd).map_err(ServiceError::Generate)?;

                service_api
                    .create(&PostParams::default(), &service)
                    .await
                    .map_err(ServiceError::Apply)?;

                tracing::info!("Service created.");
            }
        }

        Ok(None)
    }

    async fn set_deployed_status(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, DeployedStatusError> {
        tracing::info!("Setting status.");

        let name = crd.name_any();
        let api = &self.api;

        let mut crd_with_status = api
            .get_status(&name)
            .await
            .map_err(DeployedStatusError::Kube)?;

        let status = OpenFaasFunctionStatus::Ok(OpenFaasFunctionOkStatus::Deployed);

        self.replace_status(&mut crd_with_status, status)
            .await
            .map_err(DeployedStatusError::Status)?;

        Ok(None)
    }
}

pub struct Operator {
    inner: Arc<OperatorInner>,
}

impl Operator {
    pub fn new(client: KubeClient, functions_namespace: String) -> Self {
        let inner = Arc::new(OperatorInner::new(client, functions_namespace));

        Self { inner }
    }

    pub async fn run(self) {
        tracing::info!("Controller starting.");

        let reconcile_span = trace_span!("Reconcile");

        let api = self.inner.api.clone();
        let deployment_api = self.inner.deployment_api.clone();
        let service_api = self.inner.service_api.clone();

        Controller::new(api, Config::default())
            .owns(deployment_api, Config::default())
            .owns(service_api, Config::default())
            .shutdown_on_signal()
            .run(reconcile, on_error, self.inner)
            .for_each(|reconciliation_result| async move {
                match reconciliation_result {
                    Ok(_) => {
                        tracing::info!("Reconciliation successful.");
                    }
                    Err(error) => {
                        tracing::error!(%error, "Reconciliation failed.");
                    }
                }
            })
            .instrument(reconcile_span)
            .await;

        tracing::info!("Controller terminated.");
    }
}

async fn reconcile(
    crd: Arc<OpenFaaSFunction>,
    context: Arc<OperatorInner>,
) -> Result<Action, ReconcileError> {
    context.reconcile(crd).await
}

fn on_error(
    _openfaas_function: Arc<OpenFaaSFunction>,
    error: &ReconcileError,
    _context: Arc<OperatorInner>,
) -> Action {
    tracing::error!(%error, "Reconciliation failed. Requeuing.");

    Action::requeue(Duration::from_secs(10))
}
