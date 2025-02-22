use std::fmt;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use quickfix::*;
use quickfix_msg44::NewOrderSingle;
use quickfix_msg44::field_types::OrdType;
use quickfix_msg44::field_types::Side;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::fantasy::RequestMessage;
use crate::shared_data;
use crate::shared_data::SharedData;

#[derive(Debug, PartialEq)]
pub enum QuickFixState {
    Create,
    Logon,
    Logout,
}

pub struct Order {
    name: String,
    age: u32,
}

pub struct FixApplication {
    sender: mpsc::UnboundedSender<QuickFixState>,
    notice_sender: Arc<Mutex<SharedData>>,
}

impl FixApplication {
    pub fn new(
        sender: mpsc::UnboundedSender<QuickFixState>,
        notice_sender: Arc<Mutex<SharedData>>,
    ) -> FixApplication {
        FixApplication {
            sender,
            notice_sender,
        }
    }

    pub fn send_message(&self, message: QuickFixState) {
        if let Err(_) = self.sender.send(message) {
            println!("receiver dropped");
            return;
        }
    }
}

impl ApplicationCallback for FixApplication {
    /// On session created.
    fn on_create(&self, session: &SessionId) {
        self.send_message(QuickFixState::Create);
    }

    /// On session logon.
    fn on_logon(&self, session: &SessionId) {
        println!("on_logon");
        self.send_message(QuickFixState::Logon);
    }

    /// On session logout.
    fn on_logout(&self, session: &SessionId) {
        println!("on_logout");
        self.send_message(QuickFixState::Logout);
    }

    /// Called before sending message to admin level.
    ///
    /// Message can be updated at this stage.
    fn on_msg_to_admin(&self, msg: &mut Message, session: &SessionId) {
        println!("on_msg_to_admin");
    }

    /// Called before sending message to application level.
    ///
    /// Message can be updated at this stage.
    fn on_msg_to_app(&self, msg: &mut Message, session: &SessionId) -> Result<(), MsgToAppError> {
        println!("on_msg_to_app");
        Ok(())
    }

    /// Called after received a message from admin level.
    fn on_msg_from_admin(
        &self,
        msg: &Message,
        session: &SessionId,
    ) -> Result<(), MsgFromAdminError> {
        println!("on_msg_from_admin");
        Ok(())
    }

    /// Called after received a message from application level.
    fn on_msg_from_app(&self, msg: &Message, session: &SessionId) -> Result<(), MsgFromAppError> {
        println!("on_msg_from_app");
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

    let (mut quick_fix_state_sender, mut quick_fix_state_receiver) =
        mpsc::unbounded_channel::<QuickFixState>();

    let fix_application = FixApplication::new(quick_fix_state_sender, shared_data);
    let app = Application::try_new(&fix_application)?;

    let mut acceptor = SocketInitiator::try_new(&settings, &app, &store_factory, &log_factory)?;
    acceptor.start()?;

    // println!(">> Wait for login sequence completion");
    // while !acceptor.is_logged_on()? {
    //     thread::sleep(Duration::from_millis(250));
    // }

    let runtime = tokio::runtime::Runtime::new().unwrap();
    println!("开始 block_on");
    runtime.block_on(async {
        let mut order_recv_chn = order_recv.lock().await;
        loop {
            tokio::select! {
                Some(msg) = quick_fix_state_receiver.recv() => {
                    // println!("从第一个通道接收到消息: {:?}", msg);
                    if msg != QuickFixState::Logon {
                        loop {
                            println!("从");
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
                    println!("从第二个通道接收到消息");
                    let now = chrono::Utc::now();
                    now.format("%Y%m%d-%T%.3f").to_string();
                    if let Ok(mut order) = NewOrderSingle::try_new(
                        request.message,
                        Side::Buy,
                        now.to_string(),
                        OrdType::Limit,
                    ) {
                        if let Err(e) = order.set_order_qty(14.0) {
                            eprintln!("Failed to set order quantity: {}", e);
                            continue;
                        }
                    
                        if let Err(e) = order.set_symbol("APPL US Equity".to_string()) {
                            eprintln!("Failed to set symbol: {}", e);
                            continue;
                        }
                    
                        if let Err(e) = order.set_price(893.123) {
                            eprintln!("Failed to set price: {}", e);
                            continue;
                        }
                    
                        if let Ok(session_id) = SessionId::try_new("FIX.4.2", "fantasy", "SIMULATOR", "") {
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
    println!("结束 block_on");
    println!(">> We are now logged on. Let's trade 🎇");
    acceptor.stop()?;
    Ok(())
}
