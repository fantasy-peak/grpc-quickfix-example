use std::fmt;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use quickfix::*;
use quickfix_msg42::Messages;
use quickfix_msg42::NewOrderSingle;
use quickfix_msg42::field_types::HandlInst;
use quickfix_msg42::field_types::OrdStatus;
use quickfix_msg42::field_types::OrdType;
use quickfix_msg42::field_types::Side;
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
    sender: mpsc::UnboundedSender<QuickFixState>,
    shared_data: Arc<Mutex<SharedData>>,
    handle: Handle,
    gw_config: GwConfig,
}

impl FixApplication {
    pub fn new(
        sender: mpsc::UnboundedSender<QuickFixState>,
        shared_data: Arc<Mutex<SharedData>>,
        handle: Handle,
        gw_config: GwConfig,
    ) -> FixApplication {
        FixApplication {
            sender,
            shared_data,
            handle,
            gw_config,
        }
    }

    pub fn send_message(&self, message: QuickFixState) {
        if let Err(_) = self.sender.send(message) {
            println!("receiver dropped");
            return;
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
    fn on_create(&self, _session: &SessionId) {
        self.send_message(QuickFixState::Create);
    }

    /// On session logon.
    fn on_logon(&self, _session: &SessionId) {
        println!("on_logon");
        self.send_message(QuickFixState::Logon);
    }

    /// On session logout.
    fn on_logout(&self, _session: &SessionId) {
        println!("on_logout");
        self.send_message(QuickFixState::Logout);
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
        println!("on_msg_from_admin");
        Ok(())
    }

    /// Called after received a message from application level.
    fn on_msg_from_app(&self, msg: &Message, _session: &SessionId) -> Result<(), MsgFromAppError> {
        match Messages::decode(msg.clone()) {
            Ok(Messages::ExecutionReport(x)) => {
                let order_id: String = x.get_order_id();
                println!("- Order ID:           {order_id}");
                self.update_cache(order_id);
            }
            Ok(msg) => println!("{msg:?}"),
            Err(err) => eprintln!("Cannot decode message: {err:?}"),
        }
        println!("===================");

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
            FantasyLogger::Stdout => writeln!(io::stdout(), "{text}"),
            FantasyLogger::Stderr => writeln!(io::stderr(), "{text}"),
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
    order_recv: Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<RequestMessage>>>,
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

    let (quick_fix_state_sender, mut quick_fix_state_receiver) =
        mpsc::unbounded_channel::<QuickFixState>();

    let fix_application = FixApplication::new(
        quick_fix_state_sender,
        shared_data,
        handle,
        gw_config.clone(),
    );
    let app = Application::try_new(&fix_application)?;

    let mut acceptor = SocketInitiator::try_new(&settings, &app, &store_factory, &log_factory)?;
    acceptor.start()?;

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut order_recv_chn = order_recv.lock().await;
        loop {
            if let Err(_) = acceptor.is_logged_on() {
                sleep(Duration::from_millis(250)).await;
                continue;
            }
            if let Some(request) = order_recv_chn.recv().await {
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
                        // let mut session = unsafe { Session::lookup(&session_id) }.unwrap();
                        // println!(
                        //     "get_expected_sender_num:{:?}",
                        //     session.get_expected_sender_num().unwrap()
                        // );
                        // println!(
                        //     "get_expected_target_num:{:?}",
                        //     session.get_expected_target_num().unwrap()
                        // );
                        // session.set_next_sender_msg_seq_num(599);
                        // session.set_next_target_msg_seq_num(888);
                        if let Err(e) = send_to_target(order.into(), &session_id) {
                            eprintln!("Failed to send order: {}", e);
                            continue;
                        }
                    } else {
                        eprintln!("Failed to create session ID");
                        continue;
                    }
                } else {
                    eprintln!("Failed to create new order");
                    continue;
                }
            }
        }
    });
    /*
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let mut order_recv_chn = order_recv.lock().await;
        loop {
            tokio::select! {
                Some(msg) = quick_fix_state_receiver.recv() => {
                    // println!("从第一个通道接收到消息: {:?}", msg);
                    if msg != QuickFixState::Logon {
                        loop {
                            if let Some(data) = quick_fix_state_receiver.recv().await{
                                if data != QuickFixState::Logon {
                                    tokio::time::sleep(Duration::from_secs(1)).await;
                                } else {
                                    println!("从第一个通道接收到消息: {:?}", data);
                                    break;
                                }
                            }
                        }
                    }
                },
                Some(request) = order_recv_chn.recv() => {
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
                        try_set!(order, set_order_qty, 14.0, "Failed to set order quantity: {}");
                        try_set!(order, set_symbol, "USDJPY".to_string(), "Failed to set symbol: {}");
                        try_set!(order, set_price, 893.123, "Failed to set price: {}");
                        try_set!(order, set_account, "fantasy".to_string(), "Failed to set account: {}");

                        if let Ok(session_id) = SessionId::try_new(
                            &gw_config.begin_string, &gw_config.sender_comp_id, &gw_config.target_comp_id, "") {
                            if let Err(e) = send_to_target(order.into(), &session_id) {
                                eprintln!("Failed to send order: {}", e);
                                continue;
                            }
                        } else {
                            eprintln!("Failed to create session ID");
                            continue;
                        }
                    } else {
                        eprintln!("Failed to create new order");
                        continue;
                    }
                },
                else => {
                }
            }
        }
    });
    */
    println!("call acceptor.stop()");
    acceptor.stop()?;
    Ok(())
}
