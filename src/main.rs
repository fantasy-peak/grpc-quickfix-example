use quickfix::*;
use tokio::sync::{Mutex, mpsc};

pub mod fix_interface;
pub use fix_interface::*;

use std::{
    env,
    io::{Read, stdin},
    process::exit,
    sync::Arc,
    thread,
    time::Duration,
};

// 自动生成的模块
pub mod fantasy {
    tonic::include_proto!("fantasy"); // 这里的包名是 proto 文件中的 package 名
}

use fantasy::example_service_server::{ExampleService, ExampleServiceServer};
use fantasy::{RequestMessage, ResponseMessage};
use futures_util::Stream; // 使用 futures_util 提供的 Stream trait
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream; // 引入 tokio_stream
use tonic::{Request, Response, Status, transport::Server};

pub struct MyExampleService {
    order_sender: mpsc::UnboundedSender<RequestMessage>,
}

impl MyExampleService {
    pub fn new(sender: mpsc::UnboundedSender<RequestMessage>) -> MyExampleService {
        MyExampleService {
            order_sender: sender,
        }
    }
}

#[tonic::async_trait]
impl ExampleService for MyExampleService {
    //    1. 一元 RPC 调用
    async fn unary_call(
        &self,
        request: Request<RequestMessage>,
    ) -> Result<Response<ResponseMessage>, Status> {
        println!("一元调用");
        // let message = request.into_inner().message;
        if let Err(_) = self.order_sender.send(request.into_inner()) {
            println!("send order error");
        }
        Ok(Response::new(ResponseMessage {
            message: format!("Hello from Unary: {}", "hello"),
        }))
    }

    // 2. 服务端流式 RPC 调用
    type ServerStreamStream = Pin<Box<dyn Stream<Item = Result<ResponseMessage, Status>> + Send>>;

    async fn server_stream(
        &self,
        request: Request<RequestMessage>,
    ) -> Result<Response<Self::ServerStreamStream>, Status> {
        println!("服务端流式调用");
        let message = request.into_inner().message;

        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            for i in 1..=5 {
                tx.send(Ok(ResponseMessage {
                    message: format!("Stream {}: {}", i, message),
                }))
                .await
                .unwrap();
            }
            println!("流结束");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    // 3. 客户端流式 RPC 调用
    async fn client_stream(
        &self,
        request: Request<tonic::Streaming<RequestMessage>>,
    ) -> Result<Response<ResponseMessage>, Status> {
        println!("客户端流式调用");
        let mut stream = request.into_inner();
        let mut messages = vec![];

        while let Some(req) = stream.message().await? {
            messages.push(req.message);
        }

        Ok(Response::new(ResponseMessage {
            message: format!("Received: {:?}", messages),
        }))
    }

    // 4. 双向流式 RPC 调用
    type BidiStreamStream = Pin<Box<dyn Stream<Item = Result<ResponseMessage, Status>> + Send>>;

    async fn bidi_stream(
        &self,
        request: Request<tonic::Streaming<RequestMessage>>,
    ) -> Result<Response<Self::BidiStreamStream>, Status> {
        println!("双向流式调用");
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(4);

        tokio::spawn(async move {
            while let Some(req) = stream.message().await.unwrap() {
                tx.send(Ok(ResponseMessage {
                    message: format!("Echo: {}", req.message),
                }))
                .await
                .unwrap();
            }
            println!("流结束");
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let Some(cfg) = args.get(1) else {
        println!("Bad program usage: {} <config_file>", args[0]);
        exit(1);
    };
    let config_file = cfg.clone();

    // recv order from grpc forward to quickfix
    let (order_sender, order_receiver) = mpsc::unbounded_channel::<RequestMessage>();
    // recv notice from quickfix, forward to grpc
    let (notice_sender, notice_receiver) = mpsc::unbounded_channel::<String>();

    let order_receiver_clone = std::sync::Arc::new(Mutex::new(order_receiver));
    let notice_sender_clone = std::sync::Arc::new(Mutex::new(notice_sender));

    thread::spawn(move || {
        let _ = start_quickfix_server(config_file.clone(), order_receiver_clone.clone());
    });

    let addr = "[::1]:50051".parse()?;
    let example_service = MyExampleService::new(order_sender);

    println!("Server listening on {}", addr);

    // 启动 gRPC 服务器
    Server::builder()
        .add_service(ExampleServiceServer::new(example_service))
        .serve(addr)
        .await?;

    Ok(())

    // // Send new order
    // let order = new_order(&single_order_sender)?;
    // let session_id = SessionId::try_new("FIX.4.4", "CLIENT1", "SERVER1", "")?;

    // println!(">> Sending order 💸");
    // send_to_target(order.into(), &session_id)?;
}
