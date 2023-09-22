use const_format::concatcp;

pub const FUNCTIONS_NAMESPACE_ENV_VAR: &str = "OPENFAAS_FUNCTIONS_NAMESPACE";
pub const FUNCTIONS_DEFAULT_NAMESPACE: &str = "openfaas-fn";

pub const GATEWAY_URL_ENV_VAR: &str = "OPENFAAS_GATEWAY_URL";
pub const GATEWAY_DEFAULT_URL: &str = "http://gateway.openfaas:8080";

pub const OPF_FO_C_UPDATE_STRATEGY_ENV_VAR: &str = "OPF_FO_C_UPDATE_STRATEGY";

pub const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub const DISPLAY_NAME: &str = "OperatoRS";

const DEFAULT_IMAGE_REPO: &str = "docker.io/jadkhaddad";

pub const DEFAULT_IMAGE_WITHOUT_TAG: &str = concatcp!(DEFAULT_IMAGE_REPO, "/", PKG_NAME);
pub const DEFAULT_IMAGE_WITH_PKG_TAG: &str = concatcp!(DEFAULT_IMAGE_WITHOUT_TAG, ":", PKG_VERSION);
pub const DEFAULT_IMAGE_WITH_LATEST_TAG: &str = concatcp!(DEFAULT_IMAGE_WITHOUT_TAG, ":latest");
