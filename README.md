# 🤖 AI-MENTOR

A high-performance, local-first AI coding mentor built in Rust. AI-Mentor leverages **Mistral** via Ollama to analyze your local codebase and autonomously generate files while keeping your data 100% private.

---

## 🚀 Features

* **Semantic Context (RAG):** A pure-Rust keyword relevance engine that feeds your specific code context into the LLM without requiring external vector databases or C++ binaries.
* **Autonomous File Generation:** The AI can create or update files in your project directory using a safe `<file name="path">` tagging system.
* **F5 Safety Gate:** Toggle **Paste Mode** with `F5`. In Paste Mode (Red Border), auto-send is disabled so you can paste large snippets. In Normal Mode (Yellow Border), `Enter` sends the prompt.
* **Privacy First:** Designed to run on `127.0.0.1`. No code ever leaves your machine.

---

## 📂 Project Structure

```text
ai-mentor-cli/
├── src/
│   ├── main.rs          # Application entry point & event loop
│   ├── app.rs           # State management & file generation logic
│   ├── ui.rs            # TUI drawing logic (Ratatui)
│   ├── mentor.rs        # Ollama API integration & streaming
│   └── rag.rs           # Keyword-based semantic search engine
├── Cargo.toml           # Project dependencies
├── config.toml          # Local LLM & endpoint configuration
└── .env.example         # Environment variable templates

```

### 📄 File Explanations

* **`src/main.rs`**: Orchestrates the terminal lifecycle. It handles keyboard events (like the `F5` toggle), manages the asynchronous MPSC channels for AI streaming, and ensures the TUI renders at 60fps.
* **`src/app.rs`**: The "brain" of the UI state. It tracks whether you are in Paste Mode, stores the AI's response as it streams, and contains the logic that parses XML tags to write new files to your disk.
* **`src/ui.rs`**: Defined the visual layout. It uses a custom RGB color set (`#FDC302`) for the branding and manages the "Thinking" indicators that appear when your CPU is processing model tokens.
* **`src/mentor.rs`**: Manages the connection to your local Ollama instance. It is hard-coded to look for `mistral` on port `11434` to ensure zero-config connectivity.
* **`src/rag.rs`**: A lightweight, pure-Rust implementation of Retrieval-Augmented Generation. It scans your local files for keywords present in your prompt to provide the AI with the most relevant code context.

---

## 🛠️ Prerequisites & Setup

1. **Ollama**: [Download here](https://ollama.com/).
2. **Pull Mistral & Embeddings**:
```powershell
ollama pull mistral:latest
ollama pull nomic-embed-text:latest
```

3. **Build & Run**:
```powershell
cargo build --release
./target/release/ai-mentor-cli.exe

```

---

## 🤝 Contributing

Contributions are what make the open-source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## 💰 Funding & Support

**AI-Mentor** is an open-source project developed by [BugMentor](https://bugmentor.com). We are dedicated to building privacy-focused, vendor-lock-in-free developer tools.


### ⚡ Direct Support (Wise)

If you prefer to support the lead developer directly with lower fees, you can scan the QR code below or use the direct link.

<a href="https://wise.com/pay/me/matiasm155">
<img src="assets/img/wise-qr.jpg" width="200" alt="Scan to pay via Wise">
</a>

**[Send a Direct Contribution via Wise](https://wise.com/pay/me/matiasm155)**

---

## ⚖️ License

Distributed under the MIT License. See `LICENSE` for more information.
