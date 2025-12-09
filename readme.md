## Getting Started

### 1. Run the Bot

```bash
cargo run
```

### 2. Setup Environment Variables

```env
TELEGRAM_BOT_TOKEN=your_bot_token_here
OPENAI_API_KEY=your_openai_api_key
```

## Commands

| Command           | Description                         |
|------------------|--------------------------------------|
| `/start`          | Start the bot & register the user    |
| `/find`           | Find cameras                         |
| `/help`           | Show available commands              |
| `/status`         | Show active devices                  |
| `/stop`           | Stop recording                       |
|-------------------|--------------------------------------|

## Tech Stack

- Rust
- Teloxide
- Tokio async runtime
- Open CV