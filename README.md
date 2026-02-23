# 🤖 AI-Mentor-CLI

A high-performance, local-first AI coding mentor built in Rust. It uses **Mistral** via Ollama to analyze your local codebase and autonomously generate files.

![Branding](https://img.shields.io/badge/Branding-%23FDC302-gold)
![Rust](https://img.shields.io/badge/Language-Rust-orange)
![LLM](https://img.shields.io/badge/LLM-Mistral-blue)

## 🚀 Features

- **Semantic Search (RAG):** A pure-Rust keyword relevance engine that feeds your local code context directly to the AI.
- **Autonomous File Creation:** The AI can write files directly to your directory using `<file name="path">` tags.
- **F5 Safety Gate:** Toggle **Paste Mode** with `F5` to prevent accidental execution when pasting large code blocks.
- **TUI Interface:** A beautiful terminal interface powered by `ratatui` with a custom Gold (#FDC302) theme.

## 🛠️ Prerequisites

1. **Ollama:** [Download Ollama](https://ollama.com/)
2. **Mistral:** Pull the latest model:
   ```powershell
   ollama pull mistral:latest
   ```

## 📦 Installation & Setup

1. **Clone the repository:**
```powershell
git clone <your-repo-url>
cd ai-mentor-cli
```

2. **Build the project:**
```powershell
cargo build --release

```

3. **Run the Mentor:**
```powershell
./target/release/ai-mentor-cli.exe

```

## 🎮 How to Use

* **Type naturally:** Ask questions about your code.
* **F5 (Paste Mode):** When the input box is **RED**, you can paste multiline snippets. Hit `F5` again to return to normal mode.
* **Enter:** Send your prompt to the mentor.
* **Esc:** Exit the application.

## ⚖️ License

Distributed under the MIT License. See `LICENSE` for more information.
