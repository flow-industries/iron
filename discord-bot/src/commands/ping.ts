import { MessageFlags, SlashCommandBuilder } from 'discord.js';
import type { Command } from '../types.js';

const command: Command = {
  data: new SlashCommandBuilder()
    .setName('ping')
    .setDescription('replies with pong'),
  async execute(interaction) {
    await interaction.reply({ content: 'pong', flags: MessageFlags.Ephemeral });
  },
};

export default command;
