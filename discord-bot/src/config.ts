import 'dotenv/config';

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`missing required env var: ${name}`);
  return value;
}

export const config = {
  DISCORD_TOKEN: required('DISCORD_TOKEN'),
  CLIENT_ID: required('CLIENT_ID'),
  GUILD_ID: process.env.GUILD_ID,
  WELCOME_CHANNEL_ID: process.env.WELCOME_CHANNEL_ID,
  WELCOME_MESSAGE:
    process.env.WELCOME_MESSAGE ?? 'welcome to the server, {user}',
} as const;
