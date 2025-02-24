use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tokio::time::Duration;
use tokio::time::sleep;

use crate::cfg::GwConfig;
use crate::order_manager::OrderManager;
use crate::shared_data::SharedData;

pub mod fantasy {
    tonic::include_proto!("fantasy"); // 这里的包名是 proto 文件中的 package 名
}

pub mod proto {
    pub(crate) const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("example_descriptor");
}

use fantasy::example_service_server::{ExampleService, ExampleServiceServer};
use fantasy::{RequestMessage, ResponseMessage};
use futures_util::Stream; // 使用 futures_util 提供的 Stream trait
use std::pin::Pin;
use tokio_stream::wrappers::ReceiverStream; // 引入 tokio_stream
use tonic::{Request, Response, Status, transport::Server};

pub struct MyExampleService {
    order_sender: mpsc::UnboundedSender<RequestMessage>,
    shared_data: Arc<Mutex<SharedData>>,
    gw_config: GwConfig,
    order_manager: Arc<Mutex<OrderManager>>,
}

impl MyExampleService {
    pub fn new(
        sender: mpsc::UnboundedSender<RequestMessage>,
        sd: Arc<tokio::sync::Mutex<SharedData>>,
        gw_cfg: GwConfig,
    ) -> MyExampleService {
        MyExampleService {
            order_sender: sender,
            shared_data: sd,
            gw_config: gw_cfg,
            order_manager: Arc::new(Mutex::new(OrderManager::new())),
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
        let request = request.into_inner();
        {
            let mut om = self.order_manager.lock().await;
            let result = om.check_and_insert(&request.message).await;
            if result {
                return Ok(Response::new(ResponseMessage {
                    message: "Duplicate order number".to_string(),
                }));
            }
        }
        // let message = request.into_inner().message;
        if let Err(_) = self.order_sender.send(request) {
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
        // let message = request.into_inner().message;
        let (tx, rx) = mpsc::channel(4000);
        let shared_data_clone = Arc::clone(&self.shared_data);
        let interval = self.gw_config.interval;

        tokio::spawn(async move {
            let mut count: u64 = 1;
            loop {
                sleep(Duration::from_millis(interval)).await;
                {
                    let data = shared_data_clone.lock().await;
                    let vec = data.get_messages_from(count);
                    for info in &vec {
                        if let Err(e) = tx
                            .send(Ok(ResponseMessage {
                                message: format!("Server Stream push: {:?}", info),
                            }))
                            .await
                        {
                            eprintln!("Failed to send message: {}", e);
                            return;
                        }
                    }
                    if let Some(last) = vec.last() {
                        count = last.counter + 1;
                    }
                }
            }
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
            while let Some(req) = match stream.message().await {
                Ok(Some(req)) => Some(req),
                Ok(None) => {
                    println!("客户端流已关闭");
                    return;
                }
                Err(e) => {
                    eprintln!("接收消息时出错: {}", e);
                    return;
                }
            } {
                if tx
                    .send(Ok(ResponseMessage {
                        message: format!("Echo: {}", req.message),
                    }))
                    .await
                    .is_err()
                {
                    eprintln!("发送消息失败，接收端可能已关闭");
                    break;
                }
            }
            println!("流结束");
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }
}
