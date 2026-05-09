//! Internal API client for k1s

use k1s_types::Resource;
use reqwest::Client;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: {status} - {message}")]
    Api { status: u16, message: String },

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type ClientResult<T> = Result<T, ClientError>;

/// k1s API client
#[derive(Clone)]
pub struct K1sClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl K1sClient {
    pub fn new(base_url: &str) -> Self {
        // Build client that accepts self-signed certs
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_default();

        Self {
            client,
            base_url: base_url.to_string(),
            token: None,
        }
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }

    fn add_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.token {
            request.header("Authorization", format!("Bearer {token}"))
        } else {
            request
        }
    }

    /// Get a cluster-scoped resource
    pub async fn get_cluster<R: Resource>(&self, name: &str) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/{}/{}",
            self.base_url,
            R::API_VERSION,
            R::PLURAL,
            name
        );

        let request = self.add_auth(self.client.get(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// List cluster-scoped resources
    pub async fn list_cluster<R: Resource>(&self) -> ClientResult<Vec<R>> {
        let url = format!(
            "{}/api/{}/{}",
            self.base_url,
            R::API_VERSION,
            R::PLURAL
        );

        let request = self.add_auth(self.client.get(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        let list: k1s_types::ResourceList<R> = response.json().await?;
        Ok(list.items)
    }

    /// Create a cluster-scoped resource
    pub async fn create_cluster<R: Resource>(&self, resource: &R) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/{}",
            self.base_url,
            R::API_VERSION,
            R::PLURAL
        );

        let request = self.add_auth(self.client.post(&url).json(resource));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// Update a cluster-scoped resource
    pub async fn update_cluster<R: Resource>(&self, name: &str, resource: &R) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/{}/{}",
            self.base_url,
            R::API_VERSION,
            R::PLURAL,
            name
        );

        let request = self.add_auth(self.client.put(&url).json(resource));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// List pods for a specific node
    pub async fn list_pods_for_node(&self, node_name: &str) -> ClientResult<Vec<k1s_types::Pod>> {
        let url = format!(
            "{}/api/v1/pods?fieldSelector=spec.nodeName={}",
            self.base_url,
            node_name
        );

        let request = self.add_auth(self.client.get(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        let list: k1s_types::ResourceList<k1s_types::Pod> = response.json().await?;
        Ok(list.items)
    }

    /// Update pod status
    pub async fn update_pod_status(&self, namespace: &str, name: &str, pod: &k1s_types::Pod) -> ClientResult<k1s_types::Pod> {
        let url = format!(
            "{}/api/v1/namespaces/{}/pods/{}/status",
            self.base_url,
            namespace,
            name
        );

        let request = self.add_auth(self.client.put(&url).json(pod));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// Get a namespaced resource
    pub async fn get<R: Resource>(&self, namespace: &str, name: &str) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/namespaces/{}/{}/{}",
            self.base_url,
            R::API_VERSION,
            namespace,
            R::PLURAL,
            name
        );

        let request = self.add_auth(self.client.get(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// List namespaced resources
    pub async fn list<R: Resource>(&self, namespace: &str) -> ClientResult<Vec<R>> {
        let url = format!(
            "{}/api/{}/namespaces/{}/{}",
            self.base_url,
            R::API_VERSION,
            namespace,
            R::PLURAL
        );

        let request = self.add_auth(self.client.get(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        let list: k1s_types::ResourceList<R> = response.json().await?;
        Ok(list.items)
    }

    /// Create a namespaced resource
    pub async fn create<R: Resource>(&self, namespace: &str, resource: &R) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/namespaces/{}/{}",
            self.base_url,
            R::API_VERSION,
            namespace,
            R::PLURAL
        );

        let request = self.add_auth(self.client.post(&url).json(resource));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// Update a namespaced resource
    pub async fn update<R: Resource>(&self, namespace: &str, name: &str, resource: &R) -> ClientResult<R> {
        let url = format!(
            "{}/api/{}/namespaces/{}/{}/{}",
            self.base_url,
            R::API_VERSION,
            namespace,
            R::PLURAL,
            name
        );

        let request = self.add_auth(self.client.put(&url).json(resource));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(response.json().await?)
    }

    /// Delete a namespaced resource
    pub async fn delete<R: Resource>(&self, namespace: &str, name: &str) -> ClientResult<()> {
        let url = format!(
            "{}/api/{}/namespaces/{}/{}/{}",
            self.base_url,
            R::API_VERSION,
            namespace,
            R::PLURAL,
            name
        );

        let request = self.add_auth(self.client.delete(&url));
        let response = request.send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(())
    }
}
