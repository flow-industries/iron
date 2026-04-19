import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('config', () => {
  beforeEach(() => {
    vi.resetModules();
    process.env.DISCORD_TOKEN = 'test-token';
    process.env.CLIENT_ID = 'test-client';
    delete process.env.GUILD_ID;
    delete process.env.WELCOME_CHANNEL_ID;
    delete process.env.WELCOME_MESSAGE;
  });

  it('loads required env vars', async () => {
    const { config } = await import('../src/config.js');
    expect(config.DISCORD_TOKEN).toBe('test-token');
    expect(config.CLIENT_ID).toBe('test-client');
  });

  it('defaults the welcome message when unset', async () => {
    const { config } = await import('../src/config.js');
    expect(config.WELCOME_MESSAGE).toBe('welcome to the server, {user}');
  });

  it('preserves a custom welcome message', async () => {
    process.env.WELCOME_MESSAGE = 'hi {user}';
    const { config } = await import('../src/config.js');
    expect(config.WELCOME_MESSAGE).toBe('hi {user}');
  });

  it('throws when DISCORD_TOKEN is missing', async () => {
    delete process.env.DISCORD_TOKEN;
    await expect(import('../src/config.js')).rejects.toThrow(/DISCORD_TOKEN/);
  });
});
