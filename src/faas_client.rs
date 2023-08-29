use crate::{
    request::functions::{DeleteFunctionRequest, FunctionDeployment, FUNCTIONS_ENDPOINT},
    util::remove_trailling_slash,
};

pub struct BasicAuth {
    username: String,
    password: String,
}

impl BasicAuth {
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
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

    pub async fn deploy_function(&self, function_deployment: FunctionDeployment) {
        let url = self.get_functions_url();

        let mut builder = self
            .client
            .post(url)
            .header("Content-Type", "application/json");

        if let Some(basic_auth) = &self.basic_auth {
            builder = builder.basic_auth(&basic_auth.username, Some(&basic_auth.password));
        }

        let body = serde_json::to_string(&function_deployment).unwrap();

        let req = builder.body(body).build().unwrap();

        let resp = self.client.execute(req).await.unwrap();

        println!("{:?}", resp);
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
