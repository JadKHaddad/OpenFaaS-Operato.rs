use crate::{
    request::functions::{DeleteFunctionRequest, FunctionDeployment, FUNCTIONS_ENDPOINT},
    util::remove_trailling_slash,
};
use reqwest::{Error as ReqwestError, StatusCode};
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

pub type FaasResult = Result<(), FaasError>;

#[derive(ThisError, Debug)]
pub enum FaasError {
    #[error("Serializing error: {0}")]
    SerializingError(
        #[source]
        #[from]
        SerdeJsonError,
    ),
    #[error("HTTP build error: {0}")]
    HttpBuilderError(#[source] ReqwestError),
    #[error("HTTP error: {0}")]
    HttpError(#[source] ReqwestError),
    #[error("Faas: bad request")]
    BadRequest,
    #[error("Faas: not found")]
    NotFound,
    #[error("Faas: internal server error")]
    InternalServerError,
    #[error("Faas: unknown status code: {0}")]
    UnknownStatusCode(u16),
}

impl From<StatusCode> for FaasError {
    fn from(status_code: StatusCode) -> Self {
        match status_code {
            StatusCode::BAD_REQUEST => FaasError::BadRequest,
            StatusCode::NOT_FOUND => FaasError::NotFound,
            StatusCode::INTERNAL_SERVER_ERROR => FaasError::InternalServerError,
            _ => FaasError::UnknownStatusCode(status_code.as_u16()),
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
            status_code => Err(status_code.into()),
        }
    }

    pub async fn deploy_function(&self, function_deployment: FunctionDeployment) -> FaasResult {
        let url = self.get_functions_url();

        let mut builder = self
            .client
            .post(url)
            .header("Content-Type", "application/json");

        if let Some(basic_auth) = &self.basic_auth {
            builder = builder.basic_auth(&basic_auth.username, Some(&basic_auth.password));
        }

        let body = serde_json::to_string(&function_deployment)?;

        let req = builder
            .body(body)
            .build()
            .map_err(FaasError::HttpBuilderError)?;

        let resp = self
            .client
            .execute(req)
            .await
            .map_err(FaasError::HttpError)?;

        Self::status_code_into_faas_result(resp.status())
    }

    pub async fn update_function(&self, function_deployment: FunctionDeployment) {
        let url = self.get_functions_url();

        let mut builder = self
            .client
            .put(url)
            .header("Content-Type", "application/json");

        if let Some(basic_auth) = &self.basic_auth {
            builder = builder.basic_auth(&basic_auth.username, Some(&basic_auth.password));
        }

        let body = serde_json::to_string(&function_deployment).unwrap();

        let req = builder.body(body).build().unwrap();

        let resp = self.client.execute(req).await.unwrap();

        println!("{:?}", resp);
    }

    pub async fn delete_function(&self, delete_function_request: DeleteFunctionRequest) {
        let url = self.get_functions_url();

        let mut builder = self
            .client
            .delete(url)
            .header("Content-Type", "application/json");

        if let Some(basic_auth) = &self.basic_auth {
            builder = builder.basic_auth(&basic_auth.username, Some(&basic_auth.password));
        }

        let body = serde_json::to_string(&delete_function_request).unwrap();

        let req = builder.body(body).build().unwrap();

        let resp = self.client.execute(req).await.unwrap();

        println!("{:?}", resp);
    }
}
