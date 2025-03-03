use log::{error, info};
use std::fmt;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crate::broker::Broker;
use crate::cfg::BrokerName;
use crate::gw_plugin::Plugin;

use fantasy_fix42::Messages;
use fantasy_fix42::NewOrderSingle;
use fantasy_fix42::field_types::HandlInst;
use fantasy_fix42::field_types::OrdStatus;
use fantasy_fix42::field_types::OrdType;
use fantasy_fix42::field_types::Side;
use quickfix::*;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::GwConfig;
use crate::server::fantasy::RequestMessage;
use crate::shared_data::SharedData;

#[derive(Debug, PartialEq)]
pub enum QuickFixState {
    Create,
    Logon,
    Logout,
}

#[derive(Debug)]
pub enum ForwardRequest {
    RequestMessage(RequestMessage),
    ErrorMessage(String),
}

pub struct FixApplication {
    shared_data: Arc<Mutex<SharedData>>,
    handle: Handle,
    gw_config: GwConfig,
    connected: Arc<AtomicBool>,
    plugin: Arc<dyn Plugin>,
}

impl FixApplication {
    pub fn new(
        shared_data: Arc<Mutex<SharedData>>,
        handle: Handle,
        gw_config: GwConfig,
        connected: Arc<AtomicBool>,
        plugin: Arc<dyn Plugin>,
    ) -> FixApplication {
        FixApplication {
            shared_data,
            handle,
            gw_config,
            connected,
            plugin,
        }
    }

    pub fn update_cache(&self, message: String) {
        let shared_data_clone = Arc::clone(&self.shared_data);
        self.handle.spawn(async move {
            let mut data = shared_data_clone.lock().await;
            data.add_message(message);
        });
    }
}

impl ApplicationCallback for FixApplication {
    /// On session created.
    fn on_create(&self, _session: &SessionId) {}

    /// On session logon.
    fn on_logon(&self, _session: &SessionId) {
        info!("on_logon");
        self.connected.store(true, Ordering::Relaxed);
    }

    /// On session logout.
    fn on_logout(&self, _session: &SessionId) {
        info!("on_logout");
        self.connected.store(false, Ordering::Relaxed);
    }

    /// Called before sending message to admin level.
    ///
    /// Message can be updated at this stage.
    fn on_msg_to_admin(&self, _msg: &mut Message, _session: &SessionId) {}

    /// Called before sending message to application level.
    ///
    /// Message can be updated at this stage.
    fn on_msg_to_app(&self, _msg: &mut Message, _session: &SessionId) -> Result<(), MsgToAppError> {
        Ok(())
    }

    /// Called after received a message from admin level.
    fn on_msg_from_admin(
        &self,
        _msg: &Message,
        _session: &SessionId,
    ) -> Result<(), MsgFromAdminError> {
        info!("on_msg_from_admin");
        Ok(())
    }

    /// Called after received a message from application level.
    fn on_msg_from_app(&self, msg: &Message, _session: &SessionId) -> Result<(), MsgFromAppError> {
        match Messages::decode(msg.clone()) {
            Ok(Messages::ExecutionReport(x)) => {
                let order_id: String = x.get_order_id();
                info!("- Order ID:           {order_id}");
                self.update_cache(order_id);
            }
            Ok(msg) => info!("{msg:?}"),
            Err(err) => error!("Cannot decode message: {err:?}"),
        }
        info!("===================");

        Ok(())
    }
}

pub enum FantasyLogger {
    /// Log to stdout.
    Stdout,
    /// Log to stderr.
    Stderr,
}

impl fmt::Debug for FantasyLogger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stdout => write!(f, "log_stdout"),
            Self::Stderr => write!(f, "log_stderr"),
        }
    }
}

impl FantasyLogger {
    fn print(&self, text: &str) {
        let text = text.replace('\x01', "|");
        let _ = match self {
            FantasyLogger::Stdout => info!("{}", text),
            FantasyLogger::Stderr => error!("{}", text),
        };
    }
}

impl LogCallback for FantasyLogger {
    fn on_incoming(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("FIX incoming: {session_id:?}: {msg}"));
    }

    fn on_outgoing(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("111111 FIX outgoing: {session_id:?}: {msg}"));
    }

    fn on_event(&self, session_id: Option<&SessionId>, msg: &str) {
        self.print(&format!("FIX event: {session_id:?}: {msg}"));
    }
}

pub fn start_quickfix_server(
    config_file: String,
    order_recv: &mut mpsc::UnboundedReceiver<ForwardRequest>,
    shared_data: Arc<tokio::sync::Mutex<SharedData>>,
    handle: Handle,
    gw_config: GwConfig,
) -> Result<(), QuickFixError> {
    /*
        let mut my_string = String::from("");
        settings.with_dictionary(None, |dict: &Dictionary| {
            my_string = dict.get::<String>("ConnectionType").unwrap()
        });
        println!("--------------------{:?}", my_string);
    */
    let settings = SessionSettings::try_from_path(config_file)?;
    let store_factory = FileMessageStoreFactory::try_new(&settings)?;
    let log_factory = LogFactory::try_new(&FantasyLogger::Stdout)?;
    let connected = Arc::new(AtomicBool::new(false));

    let plugin: Arc<dyn Plugin>;
    match &gw_config.broker_name {
        BrokerName::Broker1 => {
            info!("===============Broker1==================");
            plugin = Arc::new(Broker::new(&gw_config.plugin_cfg_file));
        }
        BrokerName::Broker2 => {
            plugin = Arc::new(Broker::new(&gw_config.plugin_cfg_file));
        }
    }

    let fix_application = FixApplication::new(
        shared_data,
        handle.clone(),
        gw_config.clone(),
        connected.clone(),
        plugin.clone(),
    );

    let app = Application::try_new(&fix_application)?;
    let mut acceptor = SocketInitiator::try_new(&settings, &app, &store_factory, &log_factory)?;
    acceptor.start()?;
    while !acceptor.is_logged_on()? {
        thread::sleep(Duration::from_millis(250));
    }

    let Ok(session_id) = SessionId::try_new(
        &gw_config.begin_string,
        &gw_config.sender_comp_id,
        &gw_config.target_comp_id,
        "",
    ) else {
        acceptor.stop()?;
        return Err(QuickFixError::invalid_argument("create session_id error"));
    };

    handle.block_on(async move {
        loop {
            if order_recv.len() == 0 || !connected.load(Ordering::Relaxed) {
                sleep(Duration::from_millis(200)).await;
                continue;
            }
            let Some(request) = order_recv.recv().await else {
                continue;
            };
            match request {
                ForwardRequest::RequestMessage(req) => {
                    println!("Received RequestMessage: {}", req.message);
                    if let Ok(order) = plugin.convert_to_new_order_single(&req) {
                        if let Err(e) = send_to_target(order.into(), &session_id) {
                            error!("Failed to send order: {}", e);
                            continue;
                        }
                    }
                }
                ForwardRequest::ErrorMessage(err) => {
                    // 匹配到 ErrorMessage 变体，处理错误信息
                    println!("Received ErrorMessage: {}", err);
                }
            }
        }
    });

    info!("call acceptor.stop()");
    acceptor.stop()?;
    Ok(())
}
