import { REST, Routes } from 'discord.js';
import { config } from './config.js';
import { logger } from './lib/logger.js';
import { loadCommands } from './loaders/commandLoader.js';

async function main(): Promise<void> {
  const commands = await loadCommands();
  const body = commands.map((c) => c.data.toJSON());
  const rest = new REST({ version: '10' }).setToken(config.DISCORD_TOKEN);

  const route = config.GUILD_ID
    ? Routes.applicationGuildCommands(config.CLIENT_ID, config.GUILD_ID)
    : Routes.applicationCommands(config.CLIENT_ID);

  const data = (await rest.put(route, { body })) as unknown[];
  const scope = config.GUILD_ID
    ? `guild ${config.GUILD_ID}`
    : 'the global scope';
  logger.info(`registered ${data.length} commands to ${scope}`);
}

main().catch((err) => {
  logger.error('deploy failed', err);
  process.exit(1);
});
