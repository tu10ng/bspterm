use acp_thread::{AgentConnection, AgentModelSelector, AgentSessionConfigOptions, AgentSessionList, AgentSessionModes, UserMessageId};
use action_log::ActionLog;
use agent_client_protocol::{self as acp};
use anyhow::{Result, anyhow};
use collections::HashMap;
use gpui::{App, AppContext as _, Entity, SharedString, Task, WeakEntity};
use project::Project;
use std::any::Any;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use terminal::{Terminal, TerminalBuilder};
use terminal::terminal_settings::TerminalSettings;
use settings::Settings as _;
use util::paths::PathStyle;
use uuid::Uuid;

use acp_thread::AcpThread;

pub struct TerminalModeConnection {
    server_name: SharedString,
    terminal: Entity<Terminal>,
    sessions: Rc<RefCell<HashMap<acp::SessionId, TerminalSession>>>,
    #[allow(dead_code)]
    root_dir: PathBuf,
}

struct TerminalSession {
    #[allow(dead_code)]
    thread: WeakEntity<AcpThread>,
}

impl TerminalModeConnection {
    pub fn new(
        server_name: SharedString,
        command: &str,
        args: &[String],
        env: Option<HashMap<String, String>>,
        root_dir: &Path,
        cx: &mut App,
    ) -> Task<Result<Self>> {
        log::info!(
            "Creating terminal mode connection for '{}' with command: {} {:?}",
            server_name,
            command,
            args
        );

        let settings = TerminalSettings::get(None, cx);
        let cwd = Some(root_dir.to_path_buf());
        let root_dir = root_dir.to_path_buf();
        let command = command.to_string();
        let args = args.to_vec();
        let env = env.unwrap_or_default();

        let task = TerminalBuilder::new(
            cwd,
            None,
            settings.shell.clone(),
            env,
            settings.cursor_shape,
            settings.alternate_scroll,
            settings.max_scroll_history_lines,
            Vec::new(),
            0,
            false,
            0,
            None,
            cx,
            Vec::new(),
            PathStyle::local(),
        );

        cx.spawn(async move |cx| {
            let builder = task.await?;
            let terminal = cx.new(|cx| builder.subscribe(cx));

            let full_command = if args.is_empty() {
                command
            } else {
                format!("{} {}", command, args.join(" "))
            };
            let command_bytes: Vec<u8> = full_command.into_bytes();
            terminal.update(cx, |term, _| {
                term.input(command_bytes);
                term.input(b"\n");
            });

            Ok(Self {
                server_name,
                terminal,
                sessions: Rc::new(RefCell::new(HashMap::default())),
                root_dir,
            })
        })
    }

    pub fn terminal(&self) -> &Entity<Terminal> {
        &self.terminal
    }
}

impl AgentConnection for TerminalModeConnection {
    fn telemetry_id(&self) -> SharedString {
        format!("{}-terminal", self.server_name).into()
    }

    fn new_session(
        self: Rc<Self>,
        project: Entity<Project>,
        _cwd: &Path,
        cx: &mut App,
    ) -> Task<Result<Entity<AcpThread>>> {
        let session_id = acp::SessionId::new(Uuid::new_v4().to_string());
        let action_log = cx.new(|_| ActionLog::new(project.clone()));

        let thread = cx.new(|cx| {
            AcpThread::new(
                None,
                self.server_name.clone(),
                self.clone(),
                project,
                action_log,
                session_id.clone(),
                watch::Receiver::constant(acp::PromptCapabilities::new()),
                cx,
            )
        });

        self.sessions.borrow_mut().insert(
            session_id,
            TerminalSession {
                thread: thread.downgrade(),
            },
        );

        Task::ready(Ok(thread))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[]
    }

    fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Err(anyhow!("Authentication not supported in terminal mode")))
    }

    fn prompt(
        &self,
        _user_message_id: Option<UserMessageId>,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        log::info!("Terminal mode: sending prompt to agent");

        let mut message_text = String::new();
        for content in params.prompt {
            if let acp::ContentBlock::Text(text) = content {
                if !message_text.is_empty() {
                    message_text.push('\n');
                }
                message_text.push_str(&text.text);
            }
        }

        if !message_text.is_empty() {
            let message_bytes: Vec<u8> = message_text.into_bytes();
            self.terminal.update(cx, move |term, _| {
                term.input(message_bytes);
                term.input(b"\n");
            });
        }

        Task::ready(Ok(acp::PromptResponse::new(acp::StopReason::EndTurn)))
    }

    fn cancel(&self, _session_id: &acp::SessionId, cx: &mut App) {
        self.terminal.update(cx, |term, _| {
            term.input(b"\x03");
        });
    }

    fn session_modes(
        &self,
        _session_id: &acp::SessionId,
        _cx: &App,
    ) -> Option<Rc<dyn AgentSessionModes>> {
        None
    }

    fn model_selector(&self, _session_id: &acp::SessionId) -> Option<Rc<dyn AgentModelSelector>> {
        None
    }

    fn session_config_options(
        &self,
        _session_id: &acp::SessionId,
        _cx: &App,
    ) -> Option<Rc<dyn AgentSessionConfigOptions>> {
        None
    }

    fn session_list(&self, _cx: &mut App) -> Option<Rc<dyn AgentSessionList>> {
        None
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}
