use crate::crds::OpenFaasFunctionSpec;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

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

impl From<OpenFaasFunctionSpec> for FunctionDeployment {
    fn from(open_faas_function_spec: OpenFaasFunctionSpec) -> Self {
        Self {
            open_faas_function_spec,
        }
    }
}

impl Deref for FunctionDeployment {
    type Target = OpenFaasFunctionSpec;

    fn deref(&self) -> &Self::Target {
        &self.open_faas_function_spec
    }
}

impl DerefMut for FunctionDeployment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.open_faas_function_spec
    }
}
