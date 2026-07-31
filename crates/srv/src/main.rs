use tonic::transport::Server;

use protos::connect::{
    ConnectRequest, ConnectResponse,
    connect_service_server::{ConnectService, ConnectServiceServer},
};

struct ConnectServer;

#[tonic::async_trait]
impl ConnectService for ConnectServer {
    async fn connect_server(
        &self,
        _request: tonic::Request<ConnectRequest>,
    ) -> Result<tonic::Response<ConnectResponse>, tonic::Status> {
        // 处理连接
        todo!()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    println!("Listening on {}.", addr);
    Server::builder()
        .add_service(ConnectServiceServer::new(ConnectServer))
        .serve(addr)
        .await?;

    Ok(())
}
