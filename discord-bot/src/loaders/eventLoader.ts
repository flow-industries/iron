import type { Client, ClientEvents } from 'discord.js';
import { readdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import type { Event } from '../types.js';

const here = dirname(fileURLToPath(import.meta.url));
const eventsDir = join(here, '..', 'events');

function isEventFile(name: string): boolean {
  return (
    (name.endsWith('.js') || name.endsWith('.ts')) &&
    !name.endsWith('.d.ts') &&
    !name.endsWith('.test.ts') &&
    !name.endsWith('.test.js')
  );
}

export async function loadEvents(client: Client): Promise<void> {
  const entries = await readdir(eventsDir, { withFileTypes: true });

  for (const entry of entries) {
    if (!entry.isFile() || !isEventFile(entry.name)) continue;
    const mod = await import(pathToFileURL(join(eventsDir, entry.name)).href);
    const event: Event | undefined = mod.default ?? mod.event;
    if (!event?.name || !event?.execute) continue;
    const listener = (...args: ClientEvents[typeof event.name]): void => {
      void event.execute(...args);
    };
    if (event.once) client.once(event.name, listener);
    else client.on(event.name, listener);
  }
}
