use std::fmt;
use std::io;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use quickfix::*;
use tokio::sync::mpsc;

use crate::fantasy::RequestMessage;

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
    notice_sender: mpsc::UnboundedSender<String>,
}

impl FixApplication {
    pub fn new(
        sender: mpsc::UnboundedSender<QuickFixState>,
        notice_sender: mpsc::UnboundedSender<String>,
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
    let (mut a, mut b) = mpsc::unbounded_channel::<String>();

    let fix_application = FixApplication::new(quick_fix_state_sender, a);
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
                Some(msg) = order_recv_chn.recv() => {
                    println!("从第二个通道接收到消息");
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
