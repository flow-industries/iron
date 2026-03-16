//! Infrastructure-as-code CLI for deploying Docker Compose apps with Caddy reverse proxy and Cloudflare DNS.

pub mod app;
pub mod caddy;
pub mod check;
pub mod cli;
pub mod cloudflare;
pub mod compose;
pub mod config;
pub mod db;
pub mod deploy;
pub mod env;
pub mod ghcr;
pub mod init;
pub mod login;
pub mod logs;
pub mod remove;
pub mod restart;
pub mod runner;
pub mod server;
pub mod ssh;
pub mod status;
pub mod stop;
pub mod ui;
pub mod update;
