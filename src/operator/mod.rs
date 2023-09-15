mod errors;

use crate::crds::defs::{
    OpenFaaSFunction, OpenFaasFunctionErrorStatus, OpenFaasFunctionOkStatus,
    OpenFaasFunctionStatus, FINALIZER_NAME,
};
use crate::operator::errors::*;
use futures::stream::StreamExt;
use k8s_openapi::api::core::v1::Namespace;
use k8s_openapi::api::{
    apps::v1::Deployment,
    core::v1::{Secret, Service},
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::api::{DeleteParams, Patch, PatchParams};
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

        let secrets_api: Api<Secret> = Api::namespaced(kubernetes_client, &functions_namespace);

        Self {
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

        let api = self.api.clone();

        let reconcile_resource_span = trace_span!("ReconcileResource", %name, %crd_namespace);
        finalizer(&api, FINALIZER_NAME, crd, |event| async move {
            match event {
                Event::Apply(crd) => self
                    .apply(crd, &crd_namespace)
                    .instrument(trace_span!("ApplyResource"))
                    .await
                    .map_err(FinalizeError::Apply),

                Event::Cleanup(crd) => self
                    .cleanup(crd, &crd_namespace)
                    .instrument(trace_span!("CleanupResource"))
                    .await
                    .map_err(FinalizeError::Cleanup),
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

        if let Some(action) = self
            .check_resource_namespace(&crd, crd_namespace)
            .instrument(trace_span!("CheckResourceNamespace", %functions_namespace))
            .await
            .map_err(ApplyError::ResourceNamespace)?
        {
            return Ok(action);
        }

        if let Some(action) = self
            .check_function_namespace(&crd)
            .instrument(trace_span!("CheckFunctionNamespace", %functions_namespace))
            .await
            .map_err(ApplyError::FunctionNamespace)?
        {
            return Ok(action);
        }

        if let Some(action) = self
            .check_deployment(&crd)
            .instrument(trace_span!("CheckDeployment"))
            .await
            .map_err(ApplyError::Deployment)?
        {
            return Ok(action);
        }

        if let Some(action) = self
            .check_service(&crd)
            .instrument(trace_span!("CheckService"))
            .await
            .map_err(ApplyError::Service)?
        {
            return Ok(action);
        }

        if let Some(action) = self
            .set_ready_status(&crd)
            .instrument(trace_span!("SetReadyStatus"))
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
                .map_err(CheckResourceNamespaceError::GetStatus)?;

            let status =
                OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::InvalidCRDNamespace);

            self.replace_status(&mut crd_with_status, status)
                .await
                .map_err(CheckResourceNamespaceError::SetStatus)?;

            tracing::info!("Awaiting change.");
            return Ok(Some(Action::await_change()));
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
                        .map_err(CheckFunctionNamespaceError::GetStatus)?;

                    let status = OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::InvalidFunctionNamespace,
                    );

                    self.replace_status(&mut crd_with_status, status)
                        .await
                        .map_err(CheckFunctionNamespaceError::SetStatus)?;

                    tracing::info!("Awaiting change.");
                    return Ok(Some(Action::await_change()));
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

        let deployment_name = crd.spec.to_name();
        let deployment_api = &self.deployment_api;

        let deployment_opt = deployment_api
            .get_opt(&deployment_name)
            .await
            .map_err(DeploymentError::Get)?;

        let crd_oref = crd
            .controller_owner_ref(&())
            .ok_or(DeploymentError::OwnerReference)?;

        match deployment_opt {
            Some(ref deployment) => {
                if let Some(action) = self
                    .check_existing_deployment(crd, &crd_oref, deployment)
                    .instrument(trace_span!("CheckExistingDeployment"))
                    .await
                    .map_err(DeploymentError::Check)?
                {
                    return Ok(Some(action));
                }
            }
            None => {
                if let Some(action) = self
                    .create_deployment(crd)
                    .instrument(trace_span!("CreateDeployment"))
                    .await
                    .map_err(DeploymentError::Create)?
                {
                    return Ok(Some(action));
                }
            }
        }

        if let Some(action) = self
            .delete_old_deployments(crd, &crd_oref)
            .instrument(trace_span!("DeleteOldDeployments"))
            .await
            .map_err(DeploymentError::Delete)?
        {
            return Ok(Some(action));
        }

        Ok(None)
    }

    async fn check_existing_deployment(
        &self,
        crd: &OpenFaaSFunction,
        crd_oref: &OwnerReference,
        deployment: &Deployment,
    ) -> Result<Option<Action>, CheckDeploymentError> {
        tracing::info!("Deployment exists. Comparing.");

        let crd_name = crd.name_any();
        let api = &self.api;
        let deployment_orefs = deployment.owner_references();

        if deployment_orefs.contains(crd_oref) {
            tracing::info!("Deployment has owner reference. Checking if ready.");

            match deployment.status {
                None => {
                    tracing::info!("Deployment has no status. Assuming not ready.");

                    let mut crd_with_status = api
                        .get_status(&crd_name)
                        .await
                        .map_err(CheckDeploymentError::GetStatus)?;

                    let status = OpenFaasFunctionStatus::Err(
                        OpenFaasFunctionErrorStatus::DeploymentNotReady,
                    );

                    self.replace_status(&mut crd_with_status, status)
                        .await
                        .map_err(CheckDeploymentError::SetStatus)?;

                    tracing::info!("Awaiting change.");
                    return Ok(Some(Action::await_change()));
                }
                Some(ref status) => match status.ready_replicas {
                    None => {
                        tracing::info!("Deployment has no ready replicas. Assuming not ready.");

                        let mut crd_with_status = api
                            .get_status(&crd_name)
                            .await
                            .map_err(CheckDeploymentError::GetStatus)?;

                        let status = OpenFaasFunctionStatus::Err(
                            OpenFaasFunctionErrorStatus::DeploymentNotReady,
                        );

                        self.replace_status(&mut crd_with_status, status)
                            .await
                            .map_err(CheckDeploymentError::SetStatus)?;

                        tracing::info!("Awaiting change.");
                        return Ok(Some(Action::await_change()));
                    }
                    Some(replicas) => {
                        tracing::info!(
                            replicas,
                            "Deployment has {replicas} ready replica(s). Assuming ready."
                        );
                    }
                },
            }
        } else {
            tracing::error!("Deployment does not have owner reference.");

            let mut crd_with_status = api
                .get_status(&crd_name)
                .await
                .map_err(CheckDeploymentError::GetStatus)?;

            let status =
                OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::DeploymentAlreadyExists);

            self.replace_status(&mut crd_with_status, status)
                .await
                .map_err(CheckDeploymentError::SetStatus)?;

            // tracing::info!("Requeueing resource.");
            // return Ok(Some(Action::requeue(Duration::from_secs(10))));

            tracing::info!("Awaiting change.");
            return Ok(Some(Action::await_change()));
        }

        crd.spec.debug_compare_deployment(deployment);

        // TODO: Compare deployment
        // needs_recreate?
        // else
        // needs_patch?
        // else ok!

        // tracing::info!("Comparing deployment for patches.");
        // if let Some(dep) = crd.spec.patch(deployment) {
        //     tracing::info!("Patching.");

        //     // replace removes old values
        //     deployment_api
        //         .replace(&deployment_name, &PostParams::default(), &dep)
        //         .await
        //         .map_err(|err| DeploymentError::Patch(PatchError::Patch(err)))?;
        // }

        // if crd.spec.deplyoment_needs_patch(&deployment) {
        //     tracing::info!("Patching.");

        //     let crd_dep = Deployment::try_from(crd)
        //         .map_err(|err| DeploymentError::Patch(PatchError::Generate(err)))?;

        //     let patch = Patch::Merge(&crd_dep);

        //     deployment_api
        //         .patch(&deployment_name, &PatchParams::default(), &patch)
        //         .await
        //         .map_err(|err| DeploymentError::Patch(PatchError::Patch(err)))?;
        // } else {
        //     tracing::info!("Deployment is up to date.");
        // }

        Ok(None)
    }

    async fn create_deployment(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, CreateDeploymentError> {
        tracing::info!("Deployment does not exist. Creating.");

        let crd_name = crd.name_any();
        let api = &self.api;
        let deployment_api = &self.deployment_api;

        if let Some(action) = self
            .check_secrets(crd)
            .instrument(trace_span!("CheckSecrets"))
            .await
            .map_err(CreateDeploymentError::Secrets)?
        {
            return Ok(Some(action));
        }

        match Deployment::try_from(crd) {
            Ok(deployment) => {
                tracing::info!("Deployment generated. Creating.");

                deployment_api
                    .create(&PostParams::default(), &deployment)
                    .await
                    .map_err(CreateDeploymentError::Apply)?;
            }
            Err(error) => {
                tracing::error!(%error, "Failed to generate deployment.");

                // Now we set the status and propagate the error
                match Option::<OpenFaasFunctionErrorStatus>::from(&error) {
                    Some(error_status) => {
                        tracing::debug!(%error, "Error can be converted to status.");

                        let mut crd_with_status = api
                            .get_status(&crd_name)
                            .await
                            .map_err(CreateDeploymentError::GetStatus)?;

                        let status = OpenFaasFunctionStatus::Err(error_status);

                        self.replace_status(&mut crd_with_status, status)
                            .await
                            .map_err(CreateDeploymentError::SetStatus)?;
                    }
                    None => {
                        tracing::debug!(%error, "Error cannot be converted to status. Skipping.");
                    }
                }

                return Err(CreateDeploymentError::Generate(error));
            }
        }

        tracing::info!("Deployment created.");

        // reque to ensure deployment is ready before deleting old ones
        // TODO: Add wait_for_ready_dep_on_name_change var.

        tracing::info!("Awaiting change.");
        Ok(Some(Action::await_change()))
    }

    async fn delete_old_deployments(
        &self,
        crd: &OpenFaaSFunction,
        crd_oref: &OwnerReference,
    ) -> Result<Option<Action>, DeleteDeploymentsError> {
        tracing::info!("Checking other deployments.");

        // deployments to be deleted are deployments with same owner reference but different name as our spec serivce (function's name)

        let deployment_name = crd.spec.to_name();
        let deployment_api = &self.deployment_api;

        for old_deployment in deployment_api
            .list(&ListParams::default())
            .await
            .map_err(DeleteDeploymentsError::List)?
            .iter()
        {
            let old_deployment_name = old_deployment.metadata.name.clone().unwrap_or_default();
            let old_deployment_orefs = old_deployment
                .metadata
                .owner_references
                .clone()
                .unwrap_or_default();

            if old_deployment_name != deployment_name && old_deployment_orefs.contains(crd_oref) {
                tracing::info!(%old_deployment_name, "Deleting old deployment.");
                deployment_api
                    .delete(&old_deployment_name, &DeleteParams::default())
                    .await
                    .map_err(DeleteDeploymentsError::Delete)?;
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
                .map_err(CheckSecretsError::List)?
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
                    .map_err(CheckSecretsError::List)?;

                let status =
                    OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::SecretsNotFound);

                self.replace_status(&mut crd_with_status, status)
                    .await
                    .map_err(CheckSecretsError::SetStatus)?;

                tracing::info!("Awaiting change.");
                return Ok(Some(Action::await_change()));
            }
        }

        tracing::info!("Secrets exist.");

        Ok(None)
    }

    async fn check_service(&self, crd: &OpenFaaSFunction) -> Result<Option<Action>, ServiceError> {
        tracing::info!("Checking if service exists.");

        let service_name = crd.spec.to_name();
        let service_api = &self.service_api;

        let service_opt = service_api
            .get_opt(&service_name)
            .await
            .map_err(ServiceError::Get)?;

        let crd_oref = crd
            .controller_owner_ref(&())
            .ok_or(ServiceError::OwnerReference)?;

        match service_opt {
            Some(ref service) => {
                if let Some(action) = self
                    .check_existing_service(crd, &crd_oref, service)
                    .instrument(trace_span!("CheckExistingService"))
                    .await
                    .map_err(ServiceError::Check)?
                {
                    return Ok(Some(action));
                }
            }
            None => {
                if let Some(action) = self
                    .create_service(crd)
                    .instrument(trace_span!("CreateService"))
                    .await
                    .map_err(ServiceError::Create)?
                {
                    return Ok(Some(action));
                }
            }
        }

        if let Some(action) = self
            .delete_old_services(crd, &crd_oref)
            .instrument(trace_span!("DeleteOldDeployments"))
            .await
            .map_err(ServiceError::Delete)?
        {
            return Ok(Some(action));
        }

        Ok(None)
    }

    async fn check_existing_service(
        &self,
        crd: &OpenFaaSFunction,
        crd_oref: &OwnerReference,
        service: &Service,
    ) -> Result<Option<Action>, CheckServiceError> {
        tracing::info!("Service exists. Comparing.");

        let crd_name = crd.name_any();
        let api = &self.api;
        let service_orefs = service.owner_references();

        if !service_orefs.contains(crd_oref) {
            tracing::error!("Service does not have owner reference.");

            let mut crd_with_status = api
                .get_status(&crd_name)
                .await
                .map_err(CheckServiceError::GetStatus)?;

            let status =
                OpenFaasFunctionStatus::Err(OpenFaasFunctionErrorStatus::ServiceAlreadyExists);

            self.replace_status(&mut crd_with_status, status)
                .await
                .map_err(CheckServiceError::SetStatus)?;

            tracing::info!("Awaiting change.");
            return Ok(Some(Action::await_change()));
        }

        // TODO: Compare service

        Ok(None)
    }

    async fn create_service(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, CreateServiceError> {
        tracing::info!("Service does not exist. Creating.");

        let service_api = &self.service_api;

        let service = Service::try_from(crd).map_err(CreateServiceError::Generate)?;

        service_api
            .create(&PostParams::default(), &service)
            .await
            .map_err(CreateServiceError::Apply)?;

        tracing::info!("Service created.");

        Ok(None)
    }

    async fn delete_old_services(
        &self,
        crd: &OpenFaaSFunction,
        crd_oref: &OwnerReference,
    ) -> Result<Option<Action>, DeleteServicesError> {
        tracing::info!("Checking other services.");

        // services to be deleted are services with same owner reference but different name as our spec serivce (function's name)

        let service_name = crd.spec.to_name();
        let service_api = &self.service_api;

        for old_service in service_api
            .list(&ListParams::default())
            .await
            .map_err(DeleteServicesError::List)?
            .iter()
        {
            let old_service_name = old_service.metadata.name.clone().unwrap_or_default();
            let old_service_orefs = old_service
                .metadata
                .owner_references
                .clone()
                .unwrap_or_default();

            if old_service_name != service_name && old_service_orefs.contains(crd_oref) {
                tracing::info!(%old_service_name, "Deleting old service.");
                service_api
                    .delete(&old_service_name, &DeleteParams::default())
                    .await
                    .map_err(DeleteServicesError::Delete)?;
            }
        }

        Ok(None)
    }

    async fn set_ready_status(
        &self,
        crd: &OpenFaaSFunction,
    ) -> Result<Option<Action>, DeployedStatusError> {
        tracing::info!("Setting status.");

        let name = crd.name_any();
        let api = &self.api;

        let mut crd_with_status = api
            .get_status(&name)
            .await
            .map_err(DeployedStatusError::GetStatus)?;

        let status = OpenFaasFunctionStatus::Ok(OpenFaasFunctionOkStatus::Ready);

        self.replace_status(&mut crd_with_status, status)
            .await
            .map_err(DeployedStatusError::SetStatus)?;

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

    pub async fn new_with_check_functions_namespace(
        client: KubeClient,
        functions_namespace: String,
    ) -> Self {
        tracing::info!("Checking if namespace exists.");
        let namespace_api: Api<Namespace> = Api::all(client.clone());

        match namespace_api.get_opt(&functions_namespace).await {
            Ok(namespace_opt) => match namespace_opt {
                Some(_) => {
                    tracing::info!("Namespace exists.");
                }
                None => {
                    tracing::warn!("Namespace does not exist.");
                }
            },
            Err(error) => {
                tracing::warn!(%error,"Failed to check if namespace exists.");
            }
        }

        Self::new(client, functions_namespace)
    }

    pub async fn run(self) {
        tracing::info!("Starting.");

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
            .await;

        tracing::info!("Terminated.");
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
