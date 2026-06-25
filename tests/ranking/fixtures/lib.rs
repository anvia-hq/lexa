pub type ProjectAgent = Agent;

pub fn create_project_agent(name: &str) -> Agent {
    Agent {
        name: name.to_string(),
        runtime: Runtime::Local,
    }
}

pub fn use_terminal_session() {
    let _ = create_project_agent("default");
}

pub struct Agent {
    pub name: String,
    pub runtime: Runtime,
}

pub enum Runtime {
    Local,
    Remote,
}

pub const DEFAULT_AGENT_NAME: &str = "agent";
