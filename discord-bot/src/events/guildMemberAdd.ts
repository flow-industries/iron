import { Events, TextChannel, type GuildMember } from 'discord.js';
import { config } from '../config.js';
import { logger } from '../lib/logger.js';
import type { Event } from '../types.js';

const event: Event<typeof Events.GuildMemberAdd> = {
  name: Events.GuildMemberAdd,
  async execute(member: GuildMember) {
    if (!config.WELCOME_CHANNEL_ID) return;

    const channel = await member.guild.channels
      .fetch(config.WELCOME_CHANNEL_ID)
      .catch(() => null);

    if (!(channel instanceof TextChannel)) {
      logger.warn('welcome channel not found or not a text channel');
      return;
    }

    const text = config.WELCOME_MESSAGE.replaceAll('{user}', `<@${member.id}>`);

    await channel.send({
      content: text,
      allowedMentions: { users: [member.id] },
    });
  },
};

export default event;
