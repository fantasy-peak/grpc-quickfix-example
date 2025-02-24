use config::{Config, Environment, File};
// grpc
use fantasy::{RequestMessage, ResponseMessage};
use server::fantasy::example_service_server::ExampleServiceServer;
use server::{fantasy, proto};
use tonic::transport::Server;

use log::{error, info};
use std::{env, io::Write, net::SocketAddr, sync::Arc, thread};
use tokio::runtime::Handle;
use tokio::sync::{Mutex, mpsc};

pub mod cfg;
pub mod fix_interface;
pub mod order_manager;
pub mod server;
pub mod shared_data;

pub use cfg::GwConfig;
pub use fix_interface::*;
pub use server::MyExampleService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .format_timestamp(None)
        .format(|buf, record| writeln!(buf, "{}", record.args()))
        .init();

    let args: Vec<_> = env::args().collect();
    let Some(cfg) = args.get(1) else {
        error!("Bad program usage: {} <config_file>", args[0]);
        return Err("Bad program usage: <config_file> argument missing".into());
    };

    // parse YAML
    let settings = Config::builder()
        .add_source(File::with_name(&cfg))
        .add_source(Environment::default().separator("_"))
        .build();
    if let Err(e) = settings {
        error!("Error loading config: {}", e);
        return Err("Error loading config".into());
    }
    let mut gw_config = None;
    let settings = settings.unwrap();
    match settings.try_deserialize::<GwConfig>() {
        Ok(config) => gw_config = Some(config),
        Err(e) => error!("Error deserializing config: {}", e),
    }
    info!("{:?}", gw_config.as_ref().unwrap());

    // recv order from grpc forward to quickfix
    let (order_sender, mut order_receiver) = mpsc::unbounded_channel::<RequestMessage>();

    let config_file = gw_config.as_ref().unwrap().fix_cfg.clone();
    let shared_data = Arc::new(Mutex::new(shared_data::SharedData::new()));
    let data_clone = shared_data.clone();
    let gw_config_clone = gw_config.as_ref().unwrap().clone();
    let handle = Handle::current();
    thread::spawn(move || {
        if let Err(e) = start_quickfix_server(
            config_file,
            &mut order_receiver,
            data_clone,
            handle,
            gw_config_clone,
        ) {
            error!("start_quickfix_server error: {}", e);
        }
    });

    let addr: SocketAddr = gw_config.as_ref().unwrap().address.parse()?;
    let example_service = MyExampleService::new(order_sender, shared_data, gw_config.unwrap());

    // https://medium.com/@drewjaja/how-to-add-grpc-reflection-with-rust-tonic-reflection-1f4e14e6750e
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    info!("Server listening on {}", addr);

    Server::builder()
        .add_service(ExampleServiceServer::new(example_service))
        .add_service(reflection_service)
        .serve(addr)
        .await?;

    Ok(())
}
