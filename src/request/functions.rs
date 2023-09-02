pub const FUNCTIONS_ENDPOINT: &str = "/system/functions";
use serde::{Deserialize, Serialize};

use crate::crds::OpenFaasFunctionSpec;

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionDeployment {
    #[serde(flatten)]
    pub open_faas_function_spec: OpenFaasFunctionSpec,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFunctionRequest {
    /// Name of deployed function
    function_name: String,
}
