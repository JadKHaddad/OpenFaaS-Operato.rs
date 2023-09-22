use crate::consts::DEFAULT_IMAGE_WITH_TAG;
use anyhow::{Context, Ok, Result as AnyResult};
use std::path::PathBuf;
use tokio::process::Command;

pub fn determin_image_for_build(image_name: String, use_package_version: bool) -> String {
    if use_package_version {
        return DEFAULT_IMAGE_WITH_TAG.to_string();
    }
    image_name
}

pub async fn build(context: PathBuf, dockerfile: PathBuf, image_name: String) -> AnyResult<()> {
    Command::new("docker")
        .env("DOCKER_BUILDKIT", "1")
        .arg("build")
        .arg("-t")
        .arg(image_name.clone())
        .arg("-f")
        .arg(dockerfile)
        .arg(context)
        .arg("--progress=plain")
        .spawn()
        .context("Build failed")?
        .wait()
        .await
        .context("Build failed")?;

    Ok(())
}

pub async fn push(image_name: String) -> AnyResult<()> {
    Command::new("docker")
        .arg("push")
        .arg(image_name)
        .spawn()
        .context("Push failed")?
        .wait()
        .await
        .context("Push failed")?;

    Ok(())
}

pub async fn build_and_push(
    context: PathBuf,
    dockerfile: PathBuf,
    image_name: String,
) -> AnyResult<()> {
    build(context, dockerfile, image_name.clone()).await?;
    push(image_name).await?;

    Ok(())
}
