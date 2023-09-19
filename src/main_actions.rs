use crate::{
    consts::DEFAULT_IMAGE,
    crds::defs::{OpenFaaSFunction, NAME},
    operator::controller::{deplyoment::DeploymentBuilder, Operator, UpdateStrategy},
};
use anyhow::{Context, Ok, Result as AnyResult};
use either::Either::Left;
use k8s_openapi::{
    api::{
        apps::v1::Deployment,
        core::v1::{Service, ServiceAccount},
        rbac::v1::{Role, RoleBinding},
    },
    apiextensions_apiserver::pkg::apis::apiextensions::v1::CustomResourceDefinition,
};
use kube::{
    api::{DeleteParams, PostParams},
    runtime::{conditions, wait::await_condition},
    Api, Client as KubeClient, CustomResourceExt, ResourceExt,
};
use std::path::PathBuf;
use tracing::{trace_span, Instrument};

pub async fn create_and_run_operator_controller(
    functions_namespace: String,
    update_strategy: UpdateStrategy,
) -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    tracing::info!(%functions_namespace, %update_strategy, "Running with current config.");

    let span = trace_span!("Create", %functions_namespace);

    let operator =
        Operator::new_with_check_functions_namespace(client, functions_namespace, update_strategy)
            .instrument(span)
            .await;

    operator.run().await;

    Ok(())
}

pub fn determin_image(image_name: String, image_version: Option<String>) -> String {
    match image_version {
        Some(image_version) => format!("{}:{}", DEFAULT_IMAGE, image_version),
        None => image_name,
    }
}

pub async fn install_operator_controller(
    deployment_builder: DeploymentBuilder,
    functions_namespace: String,
) -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let service_account_api =
        Api::<ServiceAccount>::namespaced(client.clone(), &functions_namespace);
    let service_account = ServiceAccount::from(&deployment_builder);

    let role_api = Api::<Role>::namespaced(client.clone(), &functions_namespace);
    let role = Role::from(&deployment_builder);

    let role_binding_api = Api::<RoleBinding>::namespaced(client.clone(), &functions_namespace);
    let role_binding = RoleBinding::from(&deployment_builder);

    let deployment_api = Api::<Deployment>::namespaced(client, &functions_namespace);
    let deployment = Deployment::from(&deployment_builder);

    if let Err(error) = service_account_api
        .create(&PostParams::default(), &service_account)
        .await
    {
        tracing::error!(%error, "Failed to create service account");
    }

    if let Err(error) = role_api.create(&PostParams::default(), &role).await {
        tracing::error!(%error, "Failed to create role");
    }

    if let Err(error) = role_binding_api
        .create(&PostParams::default(), &role_binding)
        .await
    {
        tracing::error!(%error, "Failed to create role binding");
    }

    if let Err(error) = deployment_api
        .create(&PostParams::default(), &deployment)
        .await
    {
        tracing::error!(%error, "Failed to create deployment");
    }

    Ok(())
}

pub async fn uninstall_operator_controller(
    deployment_builder: DeploymentBuilder,
    functions_namespace: String,
) -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let service_account_api =
        Api::<ServiceAccount>::namespaced(client.clone(), &functions_namespace);
    let service_account_name = deployment_builder.to_service_account_name();

    let role_api = Api::<Role>::namespaced(client.clone(), &functions_namespace);
    let role_name = deployment_builder.to_role_name();

    let role_binding_api = Api::<RoleBinding>::namespaced(client.clone(), &functions_namespace);
    let role_binding_name = deployment_builder.to_role_binding_name();

    let deployment_api = Api::<Deployment>::namespaced(client, &functions_namespace);
    let deployment_name = deployment_builder.to_deployment_name();

    if let Err(error) = service_account_api
        .delete(&service_account_name, &DeleteParams::default())
        .await
    {
        tracing::error!(%error, "Failed to delete service account");
    }

    if let Err(error) = role_api.delete(&role_name, &DeleteParams::default()).await {
        tracing::error!(%error, "Failed to delete role");
    }

    if let Err(error) = role_binding_api
        .delete(&role_binding_name, &DeleteParams::default())
        .await
    {
        tracing::error!(%error, "Failed to delete role binding");
    }

    if let Err(error) = deployment_api
        .delete(&deployment_name, &DeleteParams::default())
        .await
    {
        tracing::error!(%error, "Failed to delete deployment");
    }

    Ok(())
}

pub async fn apply_crd_resources(crd: OpenFaaSFunction) -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let deployment_api = Api::<Deployment>::all(client.clone());
    let service_api = Api::<Service>::all(client);

    let deployment = Deployment::try_from(&crd.spec)?;
    let service = Service::try_from(&crd.spec)?;

    if let Err(error) = deployment_api
        .create(&PostParams::default(), &deployment)
        .await
    {
        tracing::error!(%error, "Failed to create deployment");
    }

    if let Err(error) = service_api.create(&PostParams::default(), &service).await {
        tracing::error!(%error, "Failed to create service");
    }

    Ok(())
}

pub async fn delete_crd_resources(crd: OpenFaaSFunction) -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let deployment_api = Api::<Deployment>::all(client.clone());
    let service_api = Api::<Service>::all(client);

    let name = crd.spec.to_name();

    if let Err(error) = deployment_api.delete(&name, &DeleteParams::default()).await {
        tracing::error!(%error, "Failed to delete deployment");
    }

    if let Err(error) = service_api.delete(&name, &DeleteParams::default()).await {
        tracing::error!(%error, "Failed to delete service");
    }
    Ok(())
}

pub fn print_crd_resources(crd: OpenFaaSFunction) -> AnyResult<()> {
    println!("{}", crd.spec.to_yaml_string()?);
    Ok(())
}

pub async fn write_crd_resources_to_file(file: PathBuf, crd: OpenFaaSFunction) -> AnyResult<()> {
    tokio::fs::write(file, crd.spec.to_yaml_string()?)
        .await
        .context("Failed to write crd to file")?;
    Ok(())
}

pub async fn read_crd_from_file(path: PathBuf) -> AnyResult<OpenFaaSFunction> {
    let crds = tokio::fs::read_to_string(path)
        .await
        .context("Failed to read crd from file")?;
    let crd = serde_yaml::from_str(&crds).context("Failed to parse crd")?;
    Ok(crd)
}

pub fn generate_crd_yaml() -> AnyResult<String> {
    serde_yaml::to_string(&OpenFaaSFunction::crd()).context("Failed to generate crd")
}

pub fn print_crd() -> AnyResult<()> {
    println!("{}", generate_crd_yaml()?);
    Ok(())
}

pub async fn write_crd_to_file(path: PathBuf) -> AnyResult<()> {
    let crds = generate_crd_yaml()?;
    tokio::fs::write(path, crds)
        .await
        .context("Failed to write crd to file")?;
    Ok(())
}

pub async fn install_crd() -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let api = Api::<CustomResourceDefinition>::all(client);
    let _ = api
        .create(&PostParams::default(), &OpenFaaSFunction::crd())
        .await?;

    await_condition(api, NAME, conditions::is_crd_established()).await?;

    Ok(())
}

pub async fn uninstall_crd() -> AnyResult<()> {
    let client = KubeClient::try_default().await?;

    let api = Api::<CustomResourceDefinition>::all(client);

    let obj = api.delete(NAME, &Default::default()).await?;
    if let Left(o) = obj {
        match o.uid() {
            Some(uid) => {
                await_condition(api, NAME, conditions::is_deleted(&uid)).await?;
            }
            None => {
                tracing::warn!("Could not find crd's uid");
            }
        }
    }

    Ok(())
}
