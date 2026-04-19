import { Collection } from 'discord.js';
import { readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { Command } from '../types.js';

const here = dirname(fileURLToPath(import.meta.url));
const commandsDir = join(here, '..', 'commands');

function isCommandFile(name: string): boolean {
  return (
    (name.endsWith('.js') || name.endsWith('.ts')) &&
    !name.endsWith('.d.ts') &&
    !name.endsWith('.test.ts') &&
    !name.endsWith('.test.js')
  );
}

export async function loadCommands(): Promise<Collection<string, Command>> {
  const commands = new Collection<string, Command>();
  const entries = await readdir(commandsDir, { withFileTypes: true });

  for (const entry of entries) {
    if (!entry.isFile() || !isCommandFile(entry.name)) continue;
    const mod = await import(pathToFileURL(join(commandsDir, entry.name)).href);
    const command: Command | undefined = mod.default ?? mod.command;
    if (!command?.data || !command?.execute) continue;
    commands.set(command.data.name, command);
  }

  return commands;
}
