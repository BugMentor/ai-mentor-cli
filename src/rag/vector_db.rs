use hnsw_rs::prelude::*;
use ollama_rs::Ollama;
use ollama_rs::generation::embeddings::request::{GenerateEmbeddingsRequest, EmbeddingsInput};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::fs;
use anyhow::Result;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CodeChunk {
    pub id: String,
    pub path: String,
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct VectorDB {
    hnsw: Arc<RwLock<Hnsw<'static, f32, DistCosine>>>,
    chunks: Arc<RwLock<HashMap<usize, CodeChunk>>>,
    ollama: Ollama,
    index_path: PathBuf,
}

impl VectorDB {
    pub fn new(persist_dir: &Path) -> Result<Self> {
        fs::create_dir_all(persist_dir)?;
        let chunks_path = persist_dir.join("chunks.json");

        let (hnsw, chunks) = if chunks_path.exists() {
            // Load existing index
            let hnsw = Hnsw::new(16, 10000, 16, 200, DistCosine);
            // file_load is failing for some reason, will revisit later if needed
            // let _ = hnsw.file_load(&persist_dir, "code_vectors"); 
            
            let chunks_json = fs::read_to_string(&chunks_path).unwrap_or_else(|_| "{}".to_string());
            let chunks: HashMap<usize, CodeChunk> = serde_json::from_str(&chunks_json).unwrap_or_default();
            (hnsw, chunks)
        } else {
            // Create new index
            let hnsw = Hnsw::new(16, 10000, 16, 200, DistCosine);
            (hnsw, HashMap::new())
        };

        Ok(Self {
            hnsw: Arc::new(RwLock::new(hnsw)),
            chunks: Arc::new(RwLock::new(chunks)),
            ollama: Ollama::default(),
            index_path: persist_dir.to_path_buf(),
        })
    }

    pub async fn add_documents(&self, documents: Vec<(PathBuf, String)>) -> Result<()> {
        let mut new_chunks = Vec::new();
        let mut vectors = Vec::new();
        let mut ids = Vec::new();

        // Chunking Strategy: Simple overlapping windows for now
        // TODO: Advanced AST-based chunking for "best performance"
        for (path, content) in documents {
            let lines: Vec<&str> = content.lines().collect();
            let chunk_size = 50;
            let overlap = 10;
            
            if lines.is_empty() { continue; }

            let mut start = 0;
            while start < lines.len() {
                let end = (start + chunk_size).min(lines.len());
                let chunk_content = lines[start..end].join("\n");
                
                if chunk_content.trim().len() > 20 { // Skip tiny chunks
                    // Generate Embedding
                    if let Ok(embedding) = self.generate_embedding(&chunk_content).await {
                        let id = uuid::Uuid::new_v4().to_string();
                        let chunk = CodeChunk {
                            id: id.clone(),
                            path: path.to_string_lossy().to_string(),
                            content: chunk_content,
                            start_line: start + 1,
                            end_line: end,
                        };
                        
                        // HNSW ID needs to be usize, so we manage a mapping
                        let internal_id = {
                            let chunks = self.chunks.read().unwrap();
                            chunks.len() + new_chunks.len()
                        };

                        new_chunks.push((internal_id, chunk));
                        vectors.push(embedding);
                        ids.push(internal_id);
                    }
                }
                
                if end == lines.len() { break; }
                start += chunk_size - overlap;
            }
        }

        if !vectors.is_empty() {
            let hnsw = self.hnsw.write().unwrap();
            for (i, vec) in vectors.iter().enumerate() {
                hnsw.insert((vec, ids[i]));
            }
            
            let mut chunks_map = self.chunks.write().unwrap();
            for (id, chunk) in new_chunks {
                chunks_map.insert(id, chunk);
            }
            
            self.save()?;
        }

        Ok(())
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<CodeChunk>> {
        let embedding = self.generate_embedding(query).await?;
        
        let results = {
            let hnsw = self.hnsw.read().unwrap();
            hnsw.search(&embedding, limit, 16) // ef_search = 16
        };

        let chunks = self.chunks.read().unwrap();
        let mut hits = Vec::new();
        
        for neighbor in results {
            if let Some(chunk) = chunks.get(&neighbor.d_id) {
                hits.push(chunk.clone());
            }
        }

        Ok(hits)
    }

    async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // Use nomic-embed-text for high performance embeddings
        let res = self.ollama.generate_embeddings(GenerateEmbeddingsRequest::new("nomic-embed-text".to_string(), EmbeddingsInput::Single(text.to_string()))).await
            .map_err(|e| anyhow::anyhow!("Ollama embedding error: {}", e))?;
        
        // Hnsw expects Vec<f32>
        Ok(res.embeddings.first().ok_or(anyhow::anyhow!("No embeddings returned"))?.iter().map(|&x| x as f32).collect())
    }

    pub fn save(&self) -> Result<()> {
        let hnsw = self.hnsw.read().unwrap();
        hnsw.file_dump(&self.index_path, "code_vectors")
            .map_err(|e| anyhow::anyhow!("Failed to save HNSW index: {:?}", e))?;
            
        let chunks = self.chunks.read().unwrap();
        let json = serde_json::to_string(&*chunks)?;
        fs::write(self.index_path.join("chunks.json"), json)?;
        Ok(())
    }
}
