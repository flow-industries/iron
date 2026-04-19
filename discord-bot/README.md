# Discord Bot

An extensible TypeScript Discord bot built on [discord.js](https://discord.js.org) v14. Drop a file into `src/commands/` or `src/events/` and it is wired up automatically — no central registry to edit.

The first shipped feature: welcome new members when they join a server.

## Prerequisites

- Node.js 20+
- A Discord application and bot user — create one at the [Discord Developer Portal](https://discord.com/developers/applications)
- `pnpm` (or `npm` / `yarn`)

## Setup

```bash
cd discord-bot
pnpm install
cp .env.example .env
```

Fill in `.env`:

| Variable             | Required | Description                                                               |
| -------------------- | -------- | ------------------------------------------------------------------------- |
| `DISCORD_TOKEN`      | yes      | Bot token from the Developer Portal                                       |
| `CLIENT_ID`          | yes      | Application ID (OAuth2 → General)                                         |
| `GUILD_ID`           | no       | If set, slash commands register instantly to this guild (dev convenience) |
| `WELCOME_CHANNEL_ID` | no       | Channel ID for welcome messages. Omit to disable the feature              |
| `WELCOME_MESSAGE`    | no       | Template. Use `{user}` as the mention placeholder                         |

### Enable privileged intents

The welcome feature needs the `Server Members Intent`. In the Developer Portal:

1. Open your application → **Bot** tab
2. Enable **Server Members Intent** under **Privileged Gateway Intents**

### Invite the bot

Use the OAuth2 URL Generator with:

- Scopes: `bot`, `applications.commands`
- Permissions: `View Channels`, `Send Messages`, `Read Message History`

## Register slash commands

```bash
pnpm deploy
```

Guild-scoped (when `GUILD_ID` is set) registers instantly. Global commands can take up to an hour to propagate across Discord.

## Run

```bash
pnpm dev       # tsx watch, hot reload
pnpm build     # compile to dist/
pnpm start     # run compiled dist/
```

## Adding a new slash command

1. Create `src/commands/yourcommand.ts`:

   ```ts
   import { SlashCommandBuilder } from 'discord.js';
   import type { Command } from '../types.js';

   const command: Command = {
     data: new SlashCommandBuilder()
       .setName('hello')
       .setDescription('say hello'),
     async execute(interaction) {
       await interaction.reply('hi');
     },
   };

   export default command;
   ```

2. Run `pnpm deploy` to register it with Discord.
3. Restart the bot (`pnpm dev` reloads automatically).

## Adding a new event handler

1. Create `src/events/yourEvent.ts`:

   ```ts
   import { Events, type Message } from 'discord.js';
   import type { Event } from '../types.js';

   const event: Event<typeof Events.MessageCreate> = {
     name: Events.MessageCreate,
     execute(message: Message) {
       if (message.author.bot) return;
     },
   };

   export default event;
   ```

2. Restart the bot.

If the event requires a new gateway intent, add it in `src/client.ts`.

## Project layout

```
src/
  index.ts              boot the client and login
  config.ts             typed env loading and validation
  client.ts             Client with required intents
  deploy-commands.ts    register slash commands via REST API
  types.ts              Command and Event interfaces
  loaders/              auto-discover commands + events from disk
  commands/             one file per slash command
  events/               one file per gateway event
  lib/logger.ts         thin console wrapper
tests/                  vitest suites
```

## Scripts

| Script           | Purpose                              |
| ---------------- | ------------------------------------ |
| `pnpm dev`       | run with hot reload via tsx          |
| `pnpm build`     | compile TypeScript to `dist/`        |
| `pnpm start`     | run the compiled output              |
| `pnpm deploy`    | register slash commands with Discord |
| `pnpm typecheck` | verify types without emitting        |
| `pnpm lint`      | run ESLint                           |
| `pnpm format`    | run Prettier in write mode           |
| `pnpm test`      | run vitest                           |

## Troubleshooting

- **"Used disallowed intents"** — enable `Server Members Intent` in the Developer Portal.
- **Slash commands don't appear** — run `pnpm deploy`. Global commands take up to an hour; for instant updates during development, set `GUILD_ID` in `.env`.
- **Welcome message never fires** — confirm the bot can see the channel in `WELCOME_CHANNEL_ID`, the `Server Members Intent` is enabled, and the channel is a standard text channel.
