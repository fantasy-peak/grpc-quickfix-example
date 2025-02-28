use log::{error, info};
use std::fmt;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

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

pub struct FixApplication {
    shared_data: Arc<Mutex<SharedData>>,
    handle: Handle,
    gw_config: GwConfig,
    connected: Arc<AtomicBool>,
}

impl FixApplication {
    pub fn new(
        shared_data: Arc<Mutex<SharedData>>,
        handle: Handle,
        gw_config: GwConfig,
        connected: Arc<AtomicBool>,
    ) -> FixApplication {
        FixApplication {
            shared_data,
            handle,
            gw_config,
            connected,
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
    order_recv: &mut mpsc::UnboundedReceiver<RequestMessage>,
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
    let mut connected = Arc::new(AtomicBool::new(false));
    let fix_application = FixApplication::new(
        shared_data,
        handle.clone(),
        gw_config.clone(),
        connected.clone(),
    );
    let app = Application::try_new(&fix_application)?;
    let mut acceptor = SocketInitiator::try_new(&settings, &app, &store_factory, &log_factory)?;
    acceptor.start()?;
    while !acceptor.is_logged_on()? {
        thread::sleep(Duration::from_millis(250));
    }
    handle.block_on(async {
        loop {
            if order_recv.len() == 0 || !connected.load(Ordering::Relaxed) {
                sleep(Duration::from_millis(1000)).await;
                continue;
            }
            if let Some(request) = order_recv.recv().await {
                let now = chrono::Utc::now();
                let timestamp = now.format("%Y%m%d-%H:%M:%S%.3f").to_string();
                if let Ok(mut order) = NewOrderSingle::try_new(
                    request.message,
                    HandlInst::AutomatedExecutionNoIntervention,
                    "USDJPY".to_string(),
                    Side::Buy,
                    timestamp,
                    OrdType::Limit,
                ) {
                    macro_rules! try_set {
                        ($order:expr, $method:ident, $value:expr, $message:expr) => {
                            if let Err(e) = $order.$method($value) {
                                eprintln!($message, e);
                                continue;
                            }
                        };
                    }
                    try_set!(
                        order,
                        set_order_qty,
                        14.0,
                        "Failed to set order quantity: {}"
                    );
                    try_set!(
                        order,
                        set_symbol,
                        "USDJPY".to_string(),
                        "Failed to set symbol: {}"
                    );
                    try_set!(order, set_price, 893.123, "Failed to set price: {}");
                    try_set!(
                        order,
                        set_account,
                        "fantasy".to_string(),
                        "Failed to set account: {}"
                    );

                    if let Ok(session_id) = SessionId::try_new(
                        &gw_config.begin_string,
                        &gw_config.sender_comp_id,
                        &gw_config.target_comp_id,
                        "",
                    ) {
                        if let Err(e) = send_to_target(order.into(), &session_id) {
                            error!("Failed to send order: {}", e);
                            continue;
                        }
                    } else {
                        error!("Failed to create session ID");
                        continue;
                    }
                } else {
                    error!("Failed to create new order");
                    continue;
                }
            }
        }
    });
    info!("call acceptor.stop()");
    acceptor.stop()?;
    Ok(())
}
