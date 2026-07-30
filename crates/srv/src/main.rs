use tonic::transport::Server;

use protos::connect::{
    ConnectRequest, ConnectResponse,
    connect_service_server::{ConnectService, ConnectServiceServer},
};

struct MyConnectService;

#[tonic::async_trait]
impl ConnectService for MyConnectService {
    async fn connect_server(
        &self,
        request: tonic::Request<ConnectRequest>,
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
        .add_service(ConnectServiceServer::new(MyConnectService))
        .serve(addr)
        .await?;

    Ok(())
}
