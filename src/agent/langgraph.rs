use async_trait::async_trait;
use ollama_rs::generation::chat::ChatMessage;
use anyhow::Result;
use std::collections::HashMap;
use tokio::time::{timeout, Duration};

/// The central state of the LangGraph agent.
#[derive(Clone, Debug)]
pub struct AgentState {
    pub task_id: u64,
    pub original_task: String,
    pub plan: Vec<String>,
    pub current_step_idx: usize,
    pub context: String,
    pub history: Vec<ChatMessage>,
    pub last_output: String,
    pub error: Option<String>,
    pub retry_count: usize,
    pub is_finished: bool,
    pub os: String,
    pub shell: String,
}

impl AgentState {
    pub fn new(task_id: u64, task: String, os: String, shell: String) -> Self {
        Self {
            task_id,
            original_task: task,
            plan: Vec::new(),
            current_step_idx: 0,
            context: String::new(),
            history: Vec::new(),
            last_output: String::new(),
            error: None,
            retry_count: 0,
            is_finished: false,
            os,
            shell,
        }
    }
}

/// A node in the graph that processes and modifies the state.
#[async_trait]
pub trait Node: Send + Sync {
    async fn run(&self, state: &mut AgentState) -> Result<()>;
}

/// Manages the nodes and transitions between them.
pub struct StateGraph {
    nodes: HashMap<String, Box<dyn Node>>,
    edges: HashMap<String, String>, // Simple direct edges for now
    conditional_edges: HashMap<String, Box<dyn Fn(&AgentState) -> String + Send + Sync>>,
}

impl StateGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            conditional_edges: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, name: &str, node: Box<dyn Node>) {
        self.nodes.insert(name.to_string(), node);
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.insert(from.to_string(), to.to_string());
    }

    pub fn add_conditional_edge<F>(&mut self, from: &str, f: F) 
    where F: Fn(&AgentState) -> String + Send + Sync + 'static 
    {
        self.conditional_edges.insert(from.to_string(), Box::new(f));
    }

    pub async fn run(&self, initial_state: &mut AgentState, start_node: &str) -> Result<()> {
        let mut current_node_name = start_node.to_string();

        loop {
            // 1. Get current node
            let node = self.nodes.get(&current_node_name)
                .ok_or_else(|| anyhow::anyhow!("Node '{}' not found in graph", current_node_name))?;

            // 2. Execute node logic with hard timeout
            match timeout(Duration::from_secs(300), node.run(initial_state)).await {
                Ok(result) => result?,
                Err(_) => {
                    initial_state.error = Some("Node execution timed out after 300 seconds.".to_string());
                    return Err(anyhow::anyhow!("Agent execution timed out. Please try a simpler task or check your environment. This usually happens if the model is slow or gets stuck in a logic loop."));
                }
            }

            if initial_state.is_finished {
                break;
            }

            // 3. Determine next node
            let next_node = if let Some(cond_fn) = self.conditional_edges.get(&current_node_name) {
                cond_fn(initial_state)
            } else if let Some(direct) = self.edges.get(&current_node_name) {
                direct.clone()
            } else {
                break; // Terminal node
            };

            if next_node == "end" || !self.nodes.contains_key(&next_node) {
                break;
            }

            current_node_name = next_node;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestNode {
        name: String,
    }

    #[async_trait]
    impl Node for TestNode {
        async fn run(&self, state: &mut AgentState) -> Result<()> {
            state.last_output.push_str(&self.name);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_state_graph_basic() {
        let mut graph = StateGraph::new();
        graph.add_node("A", Box::new(TestNode { name: "A".to_string() }));
        graph.add_node("B", Box::new(TestNode { name: "B".to_string() }));
        graph.add_edge("A", "B");

        let mut state = AgentState::new(1, "test".to_string(), "test_os".to_string(), "test_shell".to_string());
        graph.run(&mut state, "A").await.unwrap();

        assert_eq!(state.last_output, "AB");
    }

    #[tokio::test]
    async fn test_state_graph_conditional() {
        let mut graph = StateGraph::new();
        graph.add_node("start", Box::new(TestNode { name: "S".to_string() }));
        graph.add_node("true_branch", Box::new(TestNode { name: "T".to_string() }));
        graph.add_node("false_branch", Box::new(TestNode { name: "F".to_string() }));

        graph.add_conditional_edge("start", |state| {
            if state.original_task == "true" {
                "true_branch".to_string()
            } else {
                "false_branch".to_string()
            }
        });

        let mut state_true = AgentState::new(1, "true".to_string(), "os".to_string(), "sh".to_string());
        graph.run(&mut state_true, "start").await.unwrap();
        assert_eq!(state_true.last_output, "ST");

        let mut state_false = AgentState::new(2, "false".to_string(), "os".to_string(), "sh".to_string());
        graph.run(&mut state_false, "start").await.unwrap();
        assert_eq!(state_false.last_output, "SF");
    }
}
