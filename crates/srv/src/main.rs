use tonic::transport::Server;

use protos::connect::{
    ConnectRequest, ConnectResponse, ConnectStatus,
    connect_service_server::{ConnectService, ConnectServiceServer},
};

struct ConnectServer;

#[tonic::async_trait]
impl ConnectService for ConnectServer {
    async fn connect_server(
        &self,
        request: tonic::Request<ConnectRequest>,
    ) -> Result<tonic::Response<ConnectResponse>, tonic::Status> {
        // 打印收到的请求信息（方便调试）
        let req = request.get_ref();
        println!("Received connection request:");
        println!("  Program version: {}", req.prog_vers);
        println!("  Sync keys count: {}", req.sync_keys.len());
        if let Some(system_info) = &req.system_info {
            println!("  System Info:");
            println!("    Name: {}", system_info.name);
            println!("    Arch: {}", system_info.arch);
            println!("    Bits: {}", system_info.bits);
            println!("    Version: {}", system_info.vers);
        } else {
            println!("  System Info: None");
        }

        // 默认返回成功
        let response = ConnectResponse {
            status: ConnectStatus::ConnectSuccess as i32,
            message: Some("Connection established successfully.".to_string()),
        };

        Ok(tonic::Response::new(response))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    println!("gRPC server listening on {}", addr);
    
    Server::builder()
        .add_service(ConnectServiceServer::new(ConnectServer))
        .serve(addr)
        .await?;

    Ok(())
}