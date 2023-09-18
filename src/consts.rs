pub const FUNCTIONS_NAMESPACE_ENV_VAR: &str = "OPENFAAS_FUNCTIONS_NAMESPACE";
pub const FUNCTIONS_DEFAULT_NAMESPACE: &str = "openfaas-fn";

pub const GATEWAY_URL_ENV_VAR: &str = "OPENFAAS_GATEWAY_URL";
pub const GATEWAY_DEFAULT_URL: &str = "http://gateway.openfaas:8080";

pub const OPFOC_UPDATE_STRATEGY_ENV_VAR: &str = "OPFOC_UPDATE_STRATEGY";

pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub const DEFAULT_IMAGE: &str = concat!("docker.io/jadkhaddad/", env!("CARGO_PKG_NAME"));

pub const DEFAULT_IMAGE_WITH_TAG: &str = concat!(
    "docker.io/jadkhaddad/",
    env!("CARGO_PKG_NAME"),
    ":",
    env!("CARGO_PKG_VERSION")
);
