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
use std::process::Stdio;
use std::rc::Rc;
use task::ShellBuilder;
use terminal::terminal_settings::TerminalSettings;
use settings::Settings as _;
use util::process::Child;
use uuid::Uuid;

use acp_thread::AcpThread;

pub struct PromptModeConnection {
    server_name: SharedString,
    command: PathBuf,
    args: Vec<String>,
    env: HashMap<String, String>,
    root_dir: PathBuf,
    sessions: Rc<RefCell<HashMap<acp::SessionId, PromptSession>>>,
}

struct PromptSession {
    #[allow(dead_code)]
    thread: WeakEntity<AcpThread>,
}

impl PromptModeConnection {
    pub fn new(
        server_name: SharedString,
        command: PathBuf,
        args: Vec<String>,
        env: HashMap<String, String>,
        root_dir: PathBuf,
    ) -> Self {
        log::info!(
            "Creating prompt mode connection for '{}' with command: {:?} {:?}",
            server_name,
            command,
            args
        );

        Self {
            server_name,
            command,
            args,
            env,
            root_dir,
            sessions: Rc::new(RefCell::new(HashMap::default())),
        }
    }
}

impl AgentConnection for PromptModeConnection {
    fn telemetry_id(&self) -> SharedString {
        format!("{}-prompt", self.server_name).into()
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
            PromptSession {
                thread: thread.downgrade(),
            },
        );

        Task::ready(Ok(thread))
    }

    fn auth_methods(&self) -> &[acp::AuthMethod] {
        &[]
    }

    fn authenticate(&self, _method_id: acp::AuthMethodId, _cx: &mut App) -> Task<Result<()>> {
        Task::ready(Err(anyhow!("Authentication not supported in prompt mode")))
    }

    fn prompt(
        &self,
        _user_message_id: Option<UserMessageId>,
        params: acp::PromptRequest,
        cx: &mut App,
    ) -> Task<Result<acp::PromptResponse>> {
        log::info!("Prompt mode: executing prompt command");

        let mut message_text = String::new();
        for content in &params.prompt {
            if let acp::ContentBlock::Text(text) = content {
                if !message_text.is_empty() {
                    message_text.push('\n');
                }
                message_text.push_str(&text.text);
            }
        }

        let command = self.command.clone();
        let args = self.args.clone();
        let env = self.env.clone();
        let root_dir = self.root_dir.clone();

        let shell = TerminalSettings::get(None, cx).shell.clone();

        cx.spawn(async move |_cx| {
            let builder = ShellBuilder::new(&shell, cfg!(windows)).non_interactive();

            let mut cmd_args = args.clone();
            cmd_args.push("--prompt".into());
            cmd_args.push(message_text);

            let mut child = builder.build_std_command(Some(command.display().to_string()), &cmd_args);
            child.envs(&env);
            child.current_dir(&root_dir);

            log::debug!("Executing prompt mode command: {:?} {:?}", command, cmd_args);

            let mut child = Child::spawn(child, Stdio::null(), Stdio::piped(), Stdio::piped())?;

            let status = child.status().await?;
            let stdout = child.stdout.take();
            let stderr = child.stderr.take();

            let output = if let Some(mut stdout) = stdout {
                use futures::AsyncReadExt;
                let mut output = String::new();
                stdout.read_to_string(&mut output).await?;
                output
            } else {
                String::new()
            };

            if let Some(mut stderr) = stderr {
                use futures::AsyncReadExt;
                let mut err_output = String::new();
                stderr.read_to_string(&mut err_output).await?;
                if !err_output.is_empty() {
                    log::warn!("Prompt mode stderr: {}", err_output.trim());
                }
            }

            if !status.success() {
                log::error!("Prompt command failed with status: {:?}", status.code());
            }

            if !output.is_empty() {
                log::info!("Prompt mode output: {}", output);
            }

            Ok(acp::PromptResponse::new(acp::StopReason::EndTurn))
        })
    }

    fn cancel(&self, _session_id: &acp::SessionId, _cx: &mut App) {
        log::info!("Prompt mode: cancel not supported (each prompt is a separate process)");
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
