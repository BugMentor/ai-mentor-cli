use anyhow::Result;
use tokio::sync::mpsc;
use crate::mentor::MentorEvent;
use crate::agent::planner::Planner;
use crate::agent::executor::Executor;
use crate::agent::reflector::Reflector;
use crate::rag::vector_db::{VectorDB, CodeChunk};
use crate::agent::langgraph::{AgentState, Node, StateGraph};
use async_trait::async_trait;
use std::sync::Arc;

pub struct Orchestrator {
    graph: StateGraph,
    tx: mpsc::Sender<MentorEvent>,
}

// --- Specific LangGraph Nodes ---

struct SearchNode {
    vector_db: Arc<VectorDB>,
    tx: mpsc::Sender<MentorEvent>,
}

#[async_trait]
impl Node for SearchNode {
    async fn run(&self, state: &mut AgentState) -> Result<()> {
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, "\n[Node: Search] Gathering codebase context via HNSW...\n".to_string())).await;
        
        let chunks: Vec<CodeChunk> = self.vector_db.search(&state.original_task, 5).await.unwrap_or_default();
        let raw_context = chunks.iter()
            .map(|c| format!("File: {}\nSnippet:\n{}\n", c.path, c.content))
            .collect::<Vec<_>>()
            .join("\n---\n");
        
        // Absolute Suppression: Strip ANY benchmark triggers/results from context
        let re = regex::Regex::new(r"(?i)let\s+w\(|solve\s+h\*x|w\(-6\)|Question:|Answer:|Suppose").unwrap();
        state.context = re.replace_all(&raw_context, "[CONTENT STRIPPED]").to_string();
        
        Ok(())
    }
}

struct EnvNode {
    tx: mpsc::Sender<MentorEvent>,
}

#[async_trait]
impl Node for EnvNode {
    async fn run(&self, state: &mut AgentState) -> Result<()> {
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, "\n[Node: Environment] Probing platform constraints...\n".to_string())).await;
        
        let info = crate::tools::get_system_info().await;
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, format!("> SYSTEM CAPABILITIES DETECTED: {}\n[VERIFIED] All subsequent shell commands MUST target this environment.\n", info.replace('\n', ", ")))).await;
        
        // Update state with confirmed info
        for line in info.lines() {
            if line.starts_with("OS: ") {
                state.os = line[4..].to_string();
            } else if line.starts_with("Shell: ") {
                state.shell = line[7..].to_string();
            }
        }
        
        Ok(())
    }
}

struct PlannerNode {
    planner: Planner,
    tx: mpsc::Sender<MentorEvent>,
}

#[async_trait]
impl Node for PlannerNode {
    async fn run(&self, state: &mut AgentState) -> Result<()> {
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, "\n[Node: Planner] Creating execution strategy...\n".to_string())).await;
        
        let raw_plan = self.planner.create_plan(&state.original_task, &state.context, &state.os, &state.shell).await?;
        
        // Plan Sanitizer: Strip XML tags that might leak from the LLM
        let tag_re = regex::Regex::new(r"(?s)<tool.*?>|</tool>").unwrap();
        let benchmark_re = regex::Regex::new(r"(?i)let\s+w\(|solve\s+h\*x|w\(-6\)|Question:|Answer:|Suppose").unwrap();
        
        state.plan = raw_plan.into_iter()
            .map(|s| {
                let s = tag_re.replace_all(&s, "").to_string();
                benchmark_re.replace_all(&s, "[CLEANED]").to_string()
            })
            .filter(|s| !s.trim().is_empty() && s != "[CLEANED]")
            .collect();
            
        state.current_step_idx = 0;
        
        Ok(())
    }
}

struct ExecutorNode {
    executor: Executor,
    tx: mpsc::Sender<MentorEvent>,
}

#[async_trait]
impl Node for ExecutorNode {
    async fn run(&self, state: &mut AgentState) -> Result<()> {
        if state.current_step_idx >= state.plan.len() {
            state.is_finished = true;
            return Ok(());
        }

        let step = &state.plan[state.current_step_idx];
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, format!("\n[Node: Executor] Step {}/{}: {}\n", state.current_step_idx + 1, state.plan.len(), step))).await;

        let result = self.executor.execute_step(state.task_id, step, &state.context, &state.os, &state.shell, self.tx.clone()).await;
        
        match result {
            Ok(output) => {
                state.last_output = output.clone();
                state.error = None;
                state.retry_count = 0;
                
                if output.to_uppercase().contains("[DONE]") {
                    state.is_finished = true;
                } else {
                    state.current_step_idx += 1;
                }
            }
            Err(e) => {
                state.error = Some(e.to_string());
            }
        }
        
        Ok(())
    }
}

struct ReflectorNode {
    reflector: Reflector,
    tx: mpsc::Sender<MentorEvent>,
}

#[async_trait]
impl Node for ReflectorNode {
    async fn run(&self, state: &mut AgentState) -> Result<()> {
        let err = state.error.as_ref().cloned().unwrap_or_default();
        let step = &state.plan[state.current_step_idx];
        
        state.retry_count += 1;
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, format!("\n[Node: Reflector] Analyzing failure (Attempt {}): {}\n", state.retry_count, err))).await;
        
        let fix = self.reflector.reflect_on_failure(step, &err).await?;
        let _ = self.tx.send(MentorEvent::Chunk(state.task_id, format!("> Correction strategy: {}\n", fix))).await;
        
        // In LangGraph, we might update the plan or history here
        state.error = None; // Reset error to allow retry
        
        Ok(())
    }
}

// --- Orchestrator Implementation ---

impl Orchestrator {
    pub async fn new(tx: mpsc::Sender<MentorEvent>) -> Result<Self> {
        let persist_dir = std::env::current_dir()?.join(".ai-mentor").join("db");
        let vector_db = Arc::new(VectorDB::new(&persist_dir)?);
        
        let mut graph = StateGraph::new();

        // Nodes
        graph.add_node("search", Box::new(SearchNode { vector_db, tx: tx.clone() }));
        graph.add_node("env", Box::new(EnvNode { tx: tx.clone() }));
        graph.add_node("planner", Box::new(PlannerNode { planner: Planner::new(), tx: tx.clone() }));
        graph.add_node("executor", Box::new(ExecutorNode { executor: Executor::new(), tx: tx.clone() }));
        graph.add_node("reflector", Box::new(ReflectorNode { reflector: Reflector::new(), tx: tx.clone() }));

        // Edges
        graph.add_edge("search", "env");
        graph.add_edge("env", "planner");
        graph.add_edge("planner", "executor");
        graph.add_edge("reflector", "executor"); // Retry on reflect

        // Conditional Edges
        graph.add_conditional_edge("executor", |state| {
            if state.error.is_some() {
                if state.retry_count >= 3 {
                    "end".to_string() // Stop retrying after 3 attempts
                } else {
                    "reflector".to_string()
                }
            } else if state.is_finished {
                "end".to_string()
            } else if state.current_step_idx < state.plan.len() && state.current_step_idx < 5 {
                "executor".to_string() // Loop back to next step
            } else {
                "end".to_string()
            }
        });

        Ok(Self { graph, tx })
    }

    pub async fn run_task(&self, task_id: u64, prompt: String, os: String, shell: String) -> Result<()> {
        let mut state = AgentState::new(task_id, prompt, os, shell);
        
        // Run the graph starting from the search node
        self.graph.run(&mut state, "search").await?;
        
        let _ = self.tx.send(MentorEvent::Finished(task_id)).await;
        Ok(())
    }
}
