import { Client, Collection, GatewayIntentBits } from 'discord.js';
import type { Command } from './types.js';

export interface BotClient extends Client {
  commands: Collection<string, Command>;
}

export function createClient(): BotClient {
  const client = new Client({
    intents: [GatewayIntentBits.Guilds, GatewayIntentBits.GuildMembers],
  }) as BotClient;
  client.commands = new Collection();
  return client;
}
