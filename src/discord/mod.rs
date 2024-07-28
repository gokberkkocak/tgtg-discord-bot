pub mod commands;
pub mod framework;

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, crate::data::DiscordData, Error>;
