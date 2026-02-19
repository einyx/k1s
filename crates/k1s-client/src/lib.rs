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
pub struct K1sClient {
    client: Client,
    base_url: String,
    token: Option<String>,
}

impl K1sClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            token: None,
        }
    }

    pub fn with_token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
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

        let response = self.client.get(&url).send().await?;

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

        let response = self.client.get(&url).send().await?;

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

        let response = self.client.post(&url).json(resource).send().await?;

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

        let response = self.client.put(&url).json(resource).send().await?;

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

        let response = self.client.delete(&url).send().await?;

        if !response.status().is_success() {
            return Err(ClientError::Api {
                status: response.status().as_u16(),
                message: response.text().await?,
            });
        }

        Ok(())
    }
}
