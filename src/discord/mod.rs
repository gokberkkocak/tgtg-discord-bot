pub mod commands;
pub mod framework;

use crate::DiscordData;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, DiscordData, Error>;
