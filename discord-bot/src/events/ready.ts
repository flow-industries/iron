import { Events, type Client } from 'discord.js';
import { logger } from '../lib/logger.js';
import type { Event } from '../types.js';

const event: Event<typeof Events.ClientReady> = {
  name: Events.ClientReady,
  once: true,
  execute(client: Client<true>) {
    logger.info(`logged in as ${client.user.tag}`);
  },
};

export default event;
