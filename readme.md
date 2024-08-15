## What is this?
`ambrog.io` is a containerisable Telegram Bot (currently in 🇮🇹 only) written in Rust.
As a bot, it executes certain operations depending on the command.

### Supported commands
- `meteo [<place>] [<dd/mm/yyyy>]`: returns a hour-by-hour weather forecast for the selected place
- `[...]languorino[...]`: asks `ambrog.io` some Ferrero® Rocher
- `audio <YT_video_id>`: returns a link with the audio of the selected YT video
- `video <YT_video_id>`: returns a link with the video of the selected YT video
- Reminder-related commands have a separate chapter due to their complexity
- (🔐 admin only) `dormi pure`: forces `ambrog.io` to shut down
- (🔐 admin only) `add <user_id>`: enables telegram `<user_id>` to talk with `ambrog.io`
- (🔐 admin only) `remove <user_id>`: disables telegram `<user_id>` to talk with `ambrog.io`
- anything else is just echoed to the enabled users

### Reminder commands
- `promemoria <ID>`: returns user's reminder with the specified numeric ID
- `promemoria miei`: returns all user's reminders' list
- `scordati <ID>`: deletes user's reminder with the specified numeric ID
- `ricordami <TIME EXPR>\n<message in new line>`: creates a reminder with the desired scheduling and the specified message
  for more info, see `Time expression` section

### Time expression
The reminder time expression is rather extensive and it needs a dedicated section to explain the extent of its potential.
TODO

### Other features
Apart from commands, the following is supported:
- listens to DockerHub webhook payloads through ngrok
- keeps all downloaded files from YT
- uses Redis for in-memory state (we can enable `--appendonly` to have persistence)
- broadcasts the update version to the configured Redis pub/sub topic

## Requirements
See the architecture files ([XML](docs/arch.xml), [SVG](docs/arch.svg), [PNG](docs/arch.png)) to have an idea of all the runtime requirements.

### Development
- `rust 2021` (see [here](https://doc.rust-lang.org/cargo/getting-started/installation.html) for installation)
- (recommended) `docker` and `docker compose`

### Runtime
- internet connection
- `yt-dlp` in path
    - see [this](https://github.com/yt-dlp/yt-dlp#installation)
- `ffmpeg` in path 
    - see [this](https://ffmpeg.org/download.html)
- a running `redis` cluster that can be reached
    - you can run `docker compose up -d` from the repo root, a redis at `localhost:6379` will be run
- a running `mongo` cluster that can be reached
    - you can run `docker compose up -d` from the repo root, a redis at `localhost:27017` will be run with `root:root`
- a `ngrok` tunnel available
    - see [step 3](https://ngrok.com/docs/getting-started/rust/#step-3-run-it)
    - see [step 4](https://ngrok.com/docs/getting-started/rust/#step-4-always-use-the-same-domain)
- a `telegram` bot
    - see [this](https://core.telegram.org/bots/tutorial)
- a directory called `storage` in the workdir (I need to make this configurable)
    - just `mkdir -p storage` from the repo root
- environment variables as described in [.env file](./.env) (write your own values):
    - `RUST_LOG=INFO`
    - `TELOXIDE_TOKEN=1208471293:somestringblablabla`
    - `NGROK_AUTHTOKEN=yourNgrokAuthToken`
    - `USER_ID=0`
    - `REDIS_URL=redis://localhost`
    - `MONGO_URL=mongodb://user:pass@localhost:27017`
    - `MONGO_DB=ambrogio`
    - `BOT_NAME=Ambrog.io`
    - `UPDATES_REDIS_TOPIC=updates`
    - `UPDATES_WEBHOOK_DOMAIN=your-ngrok-domain-name`
    - `FERRERO_GIF_URL=https://67kqts2llyhkzax72fivullwhuo7ifgux6qlfavaherscx4xv3ca.arweave.net/99UJy0teDqyC_9FRWi12PR30FNS_oLKCoDkjIV-XrsQ`
    - `FORECAST_MAIN_ROOT=https://api.open-meteo.com`
    - `FORECAST_GEO_ROOT=https://geocoding-api.open-meteo.com`

## How to run

### Locally
Assuming:
- a `.env.local` file is available with all the correct values
- [runtime requirements](#runtime) are satisfied
- `rust 2021` is installed and `cargo` can be found in `PATH` variable

Run the following:
```shell
set -o allexport ; source .env.local ; set +o allexport ; cargo run bin
```

Output will be JSON logs, I advice you to use either `jl` or `jq`.

### Dockerised
Assuming:
- a `.env.local` file is available with all the correct values
- [runtime requirements](#runtime) are satisfied
- `rust 2021` is installed and `cargo` can be found in `PATH` variable
- `docker` and `docker compose` are installed
- you're using a Linux x86 machine

Run the following:
```shell
./buildme.sh && docker compose -f docs/docker-compose-example.yml up -d
```

## Git Hooks

Run this from the repo root.

`git config --local core.hooksPath .git_custom/hooks`
