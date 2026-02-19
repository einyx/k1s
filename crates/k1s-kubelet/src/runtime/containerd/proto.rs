//! Containerd gRPC proto definitions
//!
//! These are manually defined to match containerd's API without needing
//! the full proto compilation step.

pub mod containerd {
    pub mod services {
        pub mod containers {
            pub mod v1 {
                use std::collections::HashMap;

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Container {
                    #[prost(string, tag = "1")]
                    pub id: String,
                    #[prost(map = "string, string", tag = "2")]
                    pub labels: HashMap<String, String>,
                    #[prost(string, tag = "3")]
                    pub image: String,
                    #[prost(message, optional, tag = "4")]
                    pub runtime: Option<container::Runtime>,
                    #[prost(message, optional, tag = "5")]
                    pub spec: Option<::prost_types::Any>,
                    #[prost(string, tag = "6")]
                    pub snapshotter: String,
                    #[prost(string, tag = "7")]
                    pub snapshot_key: String,
                    #[prost(message, optional, tag = "8")]
                    pub created_at: Option<::prost_types::Timestamp>,
                    #[prost(message, optional, tag = "9")]
                    pub updated_at: Option<::prost_types::Timestamp>,
                    #[prost(map = "string, string", tag = "10")]
                    pub extensions: HashMap<String, String>,
                }

                pub mod container {
                    #[derive(Clone, PartialEq, ::prost::Message)]
                    pub struct Runtime {
                        #[prost(string, tag = "1")]
                        pub name: String,
                        #[prost(message, optional, tag = "2")]
                        pub options: Option<::prost_types::Any>,
                    }
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetContainerRequest {
                    #[prost(string, tag = "1")]
                    pub id: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetContainerResponse {
                    #[prost(message, optional, tag = "1")]
                    pub container: Option<Container>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListContainersRequest {
                    #[prost(string, repeated, tag = "1")]
                    pub filters: Vec<String>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListContainersResponse {
                    #[prost(message, repeated, tag = "1")]
                    pub containers: Vec<Container>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateContainerRequest {
                    #[prost(message, optional, tag = "1")]
                    pub container: Option<Container>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateContainerResponse {
                    #[prost(message, optional, tag = "1")]
                    pub container: Option<Container>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct DeleteContainerRequest {
                    #[prost(string, tag = "1")]
                    pub id: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct UpdateContainerRequest {
                    #[prost(message, optional, tag = "1")]
                    pub container: Option<Container>,
                    #[prost(message, optional, tag = "2")]
                    pub update_mask: Option<::prost_types::FieldMask>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct UpdateContainerResponse {
                    #[prost(message, optional, tag = "1")]
                    pub container: Option<Container>,
                }

                pub mod containers_client {
                    use super::*;
                    use tonic::codegen::*;

                    #[derive(Debug, Clone)]
                    pub struct ContainersClient<T> {
                        inner: tonic::client::Grpc<T>,
                    }

                    impl ContainersClient<tonic::transport::Channel> {
                        pub fn new(channel: tonic::transport::Channel) -> Self {
                            let inner = tonic::client::Grpc::new(channel);
                            Self { inner }
                        }
                    }

                    impl<T> ContainersClient<T>
                    where
                        T: tonic::client::GrpcService<tonic::body::BoxBody>,
                        T::Error: Into<StdError>,
                        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
                        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
                    {
                        pub async fn get(
                            &mut self,
                            request: impl tonic::IntoRequest<GetContainerRequest>,
                        ) -> Result<tonic::Response<GetContainerResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.containers.v1.Containers/Get",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn list(
                            &mut self,
                            request: impl tonic::IntoRequest<ListContainersRequest>,
                        ) -> Result<tonic::Response<ListContainersResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.containers.v1.Containers/List",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn create(
                            &mut self,
                            request: impl tonic::IntoRequest<CreateContainerRequest>,
                        ) -> Result<tonic::Response<CreateContainerResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.containers.v1.Containers/Create",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn delete(
                            &mut self,
                            request: impl tonic::IntoRequest<DeleteContainerRequest>,
                        ) -> Result<tonic::Response<()>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.containers.v1.Containers/Delete",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }
                    }
                }
            }
        }

        pub mod tasks {
            pub mod v1 {
                #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
                #[repr(i32)]
                pub enum Status {
                    Unknown = 0,
                    Created = 1,
                    Running = 2,
                    Stopped = 3,
                    Paused = 4,
                    Pausing = 5,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Process {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(string, tag = "2")]
                    pub id: String,
                    #[prost(uint32, tag = "3")]
                    pub pid: u32,
                    #[prost(enumeration = "Status", tag = "4")]
                    pub status: i32,
                    #[prost(string, tag = "5")]
                    pub stdin: String,
                    #[prost(string, tag = "6")]
                    pub stdout: String,
                    #[prost(string, tag = "7")]
                    pub stderr: String,
                    #[prost(bool, tag = "8")]
                    pub terminal: bool,
                    #[prost(uint32, tag = "9")]
                    pub exit_status: u32,
                    #[prost(message, optional, tag = "10")]
                    pub exited_at: Option<::prost_types::Timestamp>,
                }

                impl Process {
                    pub fn get_status(&self) -> Status {
                        Status::try_from(self.status).unwrap_or(Status::Unknown)
                    }
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateTaskRequest {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(message, repeated, tag = "2")]
                    pub rootfs: Vec<Mount>,
                    #[prost(string, tag = "3")]
                    pub stdin: String,
                    #[prost(string, tag = "4")]
                    pub stdout: String,
                    #[prost(string, tag = "5")]
                    pub stderr: String,
                    #[prost(bool, tag = "6")]
                    pub terminal: bool,
                    #[prost(message, optional, tag = "7")]
                    pub checkpoint: Option<::prost_types::Any>,
                    #[prost(message, optional, tag = "8")]
                    pub options: Option<::prost_types::Any>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Mount {
                    #[prost(string, tag = "1")]
                    pub r#type: String,
                    #[prost(string, tag = "2")]
                    pub source: String,
                    #[prost(string, tag = "3")]
                    pub target: String,
                    #[prost(string, repeated, tag = "4")]
                    pub options: Vec<String>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateTaskResponse {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(uint32, tag = "2")]
                    pub pid: u32,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct StartRequest {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(string, tag = "2")]
                    pub exec_id: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct StartResponse {
                    #[prost(uint32, tag = "1")]
                    pub pid: u32,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct KillRequest {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(string, tag = "2")]
                    pub exec_id: String,
                    #[prost(uint32, tag = "3")]
                    pub signal: u32,
                    #[prost(bool, tag = "4")]
                    pub all: bool,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct DeleteTaskRequest {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct DeleteResponse {
                    #[prost(string, tag = "1")]
                    pub id: String,
                    #[prost(uint32, tag = "2")]
                    pub pid: u32,
                    #[prost(uint32, tag = "3")]
                    pub exit_status: u32,
                    #[prost(message, optional, tag = "4")]
                    pub exited_at: Option<::prost_types::Timestamp>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetRequest {
                    #[prost(string, tag = "1")]
                    pub container_id: String,
                    #[prost(string, tag = "2")]
                    pub exec_id: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetResponse {
                    #[prost(message, optional, tag = "1")]
                    pub process: Option<Process>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListTasksRequest {
                    #[prost(string, tag = "1")]
                    pub filter: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListTasksResponse {
                    #[prost(message, repeated, tag = "1")]
                    pub tasks: Vec<Process>,
                }

                pub mod tasks_client {
                    use super::*;
                    use tonic::codegen::*;

                    #[derive(Debug, Clone)]
                    pub struct TasksClient<T> {
                        inner: tonic::client::Grpc<T>,
                    }

                    impl TasksClient<tonic::transport::Channel> {
                        pub fn new(channel: tonic::transport::Channel) -> Self {
                            let inner = tonic::client::Grpc::new(channel);
                            Self { inner }
                        }
                    }

                    impl<T> TasksClient<T>
                    where
                        T: tonic::client::GrpcService<tonic::body::BoxBody>,
                        T::Error: Into<StdError>,
                        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
                        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
                    {
                        pub async fn create(
                            &mut self,
                            request: impl tonic::IntoRequest<CreateTaskRequest>,
                        ) -> Result<tonic::Response<CreateTaskResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/Create",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn start(
                            &mut self,
                            request: impl tonic::IntoRequest<StartRequest>,
                        ) -> Result<tonic::Response<StartResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/Start",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn kill(
                            &mut self,
                            request: impl tonic::IntoRequest<KillRequest>,
                        ) -> Result<tonic::Response<()>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/Kill",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn delete(
                            &mut self,
                            request: impl tonic::IntoRequest<DeleteTaskRequest>,
                        ) -> Result<tonic::Response<DeleteResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/Delete",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn get(
                            &mut self,
                            request: impl tonic::IntoRequest<GetRequest>,
                        ) -> Result<tonic::Response<GetResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/Get",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn list(
                            &mut self,
                            request: impl tonic::IntoRequest<ListTasksRequest>,
                        ) -> Result<tonic::Response<ListTasksResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.tasks.v1.Tasks/List",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }
                    }
                }
            }
        }

        pub mod images {
            pub mod v1 {
                use std::collections::HashMap;

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Image {
                    #[prost(string, tag = "1")]
                    pub name: String,
                    #[prost(map = "string, string", tag = "2")]
                    pub labels: HashMap<String, String>,
                    #[prost(message, optional, tag = "3")]
                    pub target: Option<Descriptor>,
                    #[prost(message, optional, tag = "4")]
                    pub created_at: Option<::prost_types::Timestamp>,
                    #[prost(message, optional, tag = "5")]
                    pub updated_at: Option<::prost_types::Timestamp>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Descriptor {
                    #[prost(string, tag = "1")]
                    pub media_type: String,
                    #[prost(string, tag = "2")]
                    pub digest: String,
                    #[prost(int64, tag = "3")]
                    pub size: i64,
                    #[prost(map = "string, string", tag = "5")]
                    pub annotations: HashMap<String, String>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListImagesRequest {
                    #[prost(string, repeated, tag = "1")]
                    pub filters: Vec<String>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct ListImagesResponse {
                    #[prost(message, repeated, tag = "1")]
                    pub images: Vec<Image>,
                }

                pub mod images_client {
                    use super::*;
                    use tonic::codegen::*;

                    #[derive(Debug, Clone)]
                    pub struct ImagesClient<T> {
                        inner: tonic::client::Grpc<T>,
                    }

                    impl ImagesClient<tonic::transport::Channel> {
                        pub fn new(channel: tonic::transport::Channel) -> Self {
                            let inner = tonic::client::Grpc::new(channel);
                            Self { inner }
                        }
                    }

                    impl<T> ImagesClient<T>
                    where
                        T: tonic::client::GrpcService<tonic::body::BoxBody>,
                        T::Error: Into<StdError>,
                        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
                        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
                    {
                        pub async fn list(
                            &mut self,
                            request: impl tonic::IntoRequest<ListImagesRequest>,
                        ) -> Result<tonic::Response<ListImagesResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.images.v1.Images/List",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }
                    }
                }
            }
        }

        pub mod content {
            pub mod v1 {
                pub mod content_client {
                    #[derive(Debug, Clone)]
                    pub struct ContentClient<T> {
                        _inner: T,
                    }

                    impl ContentClient<tonic::transport::Channel> {
                        pub fn new(_channel: tonic::transport::Channel) -> Self {
                            Self { _inner: _channel }
                        }
                    }
                }
            }
        }

        pub mod namespaces {
            pub mod v1 {
                use std::collections::HashMap;

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct Namespace {
                    #[prost(string, tag = "1")]
                    pub name: String,
                    #[prost(map = "string, string", tag = "2")]
                    pub labels: HashMap<String, String>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetNamespaceRequest {
                    #[prost(string, tag = "1")]
                    pub name: String,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct GetNamespaceResponse {
                    #[prost(message, optional, tag = "1")]
                    pub namespace: Option<Namespace>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateNamespaceRequest {
                    #[prost(message, optional, tag = "1")]
                    pub namespace: Option<Namespace>,
                }

                #[derive(Clone, PartialEq, ::prost::Message)]
                pub struct CreateNamespaceResponse {
                    #[prost(message, optional, tag = "1")]
                    pub namespace: Option<Namespace>,
                }

                pub mod namespaces_client {
                    use super::*;
                    use tonic::codegen::*;

                    #[derive(Debug, Clone)]
                    pub struct NamespacesClient<T> {
                        inner: tonic::client::Grpc<T>,
                    }

                    impl NamespacesClient<tonic::transport::Channel> {
                        pub fn new(channel: tonic::transport::Channel) -> Self {
                            let inner = tonic::client::Grpc::new(channel);
                            Self { inner }
                        }
                    }

                    impl<T> NamespacesClient<T>
                    where
                        T: tonic::client::GrpcService<tonic::body::BoxBody>,
                        T::Error: Into<StdError>,
                        T::ResponseBody: Body<Data = Bytes> + Send + 'static,
                        <T::ResponseBody as Body>::Error: Into<StdError> + Send,
                    {
                        pub async fn get(
                            &mut self,
                            request: impl tonic::IntoRequest<GetNamespaceRequest>,
                        ) -> Result<tonic::Response<GetNamespaceResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.namespaces.v1.Namespaces/Get",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }

                        pub async fn create(
                            &mut self,
                            request: impl tonic::IntoRequest<CreateNamespaceRequest>,
                        ) -> Result<tonic::Response<CreateNamespaceResponse>, tonic::Status>
                        {
                            self.inner.ready().await.map_err(|e| {
                                tonic::Status::new(tonic::Code::Unknown, format!("Service was not ready: {}", e.into()))
                            })?;
                            let codec = tonic::codec::ProstCodec::default();
                            let path = http::uri::PathAndQuery::from_static(
                                "/containerd.services.namespaces.v1.Namespaces/Create",
                            );
                            self.inner.unary(request.into_request(), path, codec).await
                        }
                    }
                }
            }
        }
    }
}
