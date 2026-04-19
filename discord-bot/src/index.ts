import { config } from './config.js';
import { createClient } from './client.js';
import { loadCommands } from './loaders/commandLoader.js';
import { loadEvents } from './loaders/eventLoader.js';
import { logger } from './lib/logger.js';

async function main(): Promise<void> {
  const client = createClient();
  client.commands = await loadCommands();
  await loadEvents(client);
  await client.login(config.DISCORD_TOKEN);
}

main().catch((err) => {
  logger.error('fatal startup error', err);
  process.exit(1);
});
