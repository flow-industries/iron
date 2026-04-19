import {
  Events,
  MessageFlags,
  type Interaction,
  type InteractionReplyOptions,
} from 'discord.js';
import type { BotClient } from '../client.js';
import { logger } from '../lib/logger.js';
import type { Event } from '../types.js';

const event: Event<typeof Events.InteractionCreate> = {
  name: Events.InteractionCreate,
  async execute(interaction: Interaction) {
    if (!interaction.isChatInputCommand()) return;

    const client = interaction.client as BotClient;
    const command = client.commands.get(interaction.commandName);
    if (!command) return;

    try {
      await command.execute(interaction);
    } catch (err) {
      logger.error(`command ${interaction.commandName} failed`, err);
      const payload: InteractionReplyOptions = {
        content: 'something went wrong',
        flags: MessageFlags.Ephemeral,
      };
      if (interaction.deferred || interaction.replied) {
        await interaction.followUp(payload);
      } else {
        await interaction.reply(payload);
      }
    }
  },
};

export default event;
