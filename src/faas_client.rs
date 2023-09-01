use crate::{
    request::functions::{DeleteFunctionRequest, FunctionDeployment, FUNCTIONS_ENDPOINT},
    util::remove_trailling_slash,
};
use reqwest::{Error as ReqwestError, Method, Request, Response, StatusCode};
use serde::Serialize;
use serde_json::Error as SerdeJsonError;
use thiserror::Error as ThisError;

pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }
}

pub type RequestBuildResult = Result<Request, RequestBuildError>;
pub type FaasResult = Result<(), FaasError>;

#[derive(ThisError, Debug)]
pub enum RequestBuildError {
    #[error("Serializing error: {0}")]
    SerializingError(
        #[source]
        #[from]
        SerdeJsonError,
    ),
    #[error("Request build error: {0}")]
    HttpBuilderError(
        #[source]
        #[from]
        ReqwestError,
    ),
}

#[derive(ThisError, Debug)]
pub enum RequestExecutionError {
    #[error("HTTP error: {0}")]
    HttpError(
        #[source]
        #[from]
        ReqwestError,
    ),
    #[error("Faas: bad request")]
    BadRequest,
    #[error("Faas: not found")]
    NotFound,
    #[error("Faas: internal server error")]
    InternalServerError,
    #[error("Faas: unexpected status code: {0}")]
    UnexpectedStatusCode(u16),
}

#[derive(ThisError, Debug)]
pub enum FaasError {
    #[error("Request build error: {0}")]
    RequestBuildError(
        #[source]
        #[from]
        RequestBuildError,
    ),
    #[error("Request execution error: {0}")]
    ExecutionError(
        #[source]
        #[from]
        RequestExecutionError,
    ),
}

impl From<StatusCode> for RequestExecutionError {
    fn from(status_code: StatusCode) -> Self {
        match status_code {
            StatusCode::BAD_REQUEST => RequestExecutionError::BadRequest,
            StatusCode::NOT_FOUND => RequestExecutionError::NotFound,
            StatusCode::INTERNAL_SERVER_ERROR => RequestExecutionError::InternalServerError,
            _ => RequestExecutionError::UnexpectedStatusCode(status_code.as_u16()),
        }
    }
}

pub struct FaasCleint {
    client: reqwest::Client,
    /// Base URL of the OpenFaaS gateway
    /// e.g. http://gateway.openfaas:8080
    base_url: String,
    basic_auth: Option<BasicAuth>,
}

impl FaasCleint {
    pub fn new(base_url: String, basic_auth: Option<BasicAuth>) -> Self {
        let base_url = remove_trailling_slash(&base_url);
        Self {
            client: reqwest::Client::new(),
            base_url,
            basic_auth,
        }
    }

    fn get_functions_url(&self) -> String {
        format!("{}{}", self.base_url, FUNCTIONS_ENDPOINT)
    }

    fn status_code_into_faas_result(status_code: StatusCode) -> FaasResult {
        match status_code {
            StatusCode::OK => Ok(()),
            StatusCode::ACCEPTED => Ok(()),
            status_code => Err(FaasError::ExecutionError(status_code.into())),
        }
    }

    pub fn build_request<T: Serialize>(&self, method: Method, body: &T) -> RequestBuildResult {
        let url = self.get_functions_url();
        let mut builder = self.client.request(method, url);
        let body = serde_json::to_string(body)?;

        builder = builder
            .header("Content-Type", "application/json")
            .body(body);

        if let Some(basic_auth) = &self.basic_auth {
            builder = builder.basic_auth(&basic_auth.username, Some(&basic_auth.password));
        }

        let req = builder.build()?;

        Ok(req)
    }

    async fn execute_request(&self, req: Request) -> Result<Response, RequestExecutionError> {
        let res = self.client.execute(req).await?;
        Ok(res)
    }

    async fn build_and_execute_request<T: Serialize>(
        &self,
        method: Method,
        body: &T,
    ) -> FaasResult {
        let req = self.build_request(method, body)?;
        let res = self.execute_request(req).await?;

        Self::status_code_into_faas_result(res.status())
    }

    pub async fn deploy_function(&self, function_deployment: FunctionDeployment) -> FaasResult {
        self.build_and_execute_request(Method::POST, &function_deployment)
            .await
    }

    pub async fn update_function(&self, function_deployment: FunctionDeployment) -> FaasResult {
        self.build_and_execute_request(Method::PUT, &function_deployment)
            .await
    }

    pub async fn delete_function(
        &self,
        delete_function_request: DeleteFunctionRequest,
    ) -> FaasResult {
        self.build_and_execute_request(Method::DELETE, &delete_function_request)
            .await
    }
}
