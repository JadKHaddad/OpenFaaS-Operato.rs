use kube::Client as KubeClient;

pub struct ContextData {
    pub(crate) kubernetes_client: KubeClient,
    pub(crate) functions_namespace: String,
}

impl ContextData {
    pub fn new(kubernetes_client: KubeClient, functions_namespace: String) -> Self {
        Self {
            kubernetes_client,
            functions_namespace,
        }
    }
}
