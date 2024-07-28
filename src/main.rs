// mod commands;
mod data;
mod db;
mod discord;
mod monitor;
mod tgtg;

use std::{collections::HashMap, env, sync::Arc, time::Duration};

use data::{DiscordData, TGTGBindings};
use discord::framework::DiscordClient;

use poise::serenity_prelude as serenity;

use serenity::all::GatewayIntents;
use tokio::sync::RwLock;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env variables if it exists.
    dotenvy::dotenv().ok();

    // Initialize the logger to use environment variables.
    tracing_subscriber::fmt::init();

    // Check python
    tgtg::check_python()?;

    let discord_token = env::var("DISCORD_TOKEN")?;
    let tgtg_access_token = env::var("TGTG_ACCESS_TOKEN")?;
    let tgtg_refresh_token = env::var("TGTG_REFRESH_TOKEN")?;
    let tgtg_user_id = env::var("TGTG_USER_ID")?;
    let tgtg_cookie = env::var("TGTG_COOKIE")?;
    let db_url = env::var("DATABASE_URL")?;

    // Bot DB
    let bot_db = Arc::new(db::BotDB::new(&db_url).await?);
    let (location_map, active_set) = bot_db.get_locations().await?;

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let active_channels = Arc::new(RwLock::new(active_set));
    let tgtg_bindings = Arc::new(TGTGBindings {
        client: crate::tgtg::init_client(
            &tgtg_access_token,
            &tgtg_refresh_token,
            &tgtg_user_id,
            &tgtg_cookie,
        )?,
        fetch_func: crate::tgtg::init_fetch_func()?,
    });
    let tgtg_configs = Arc::new(RwLock::new(location_map));
    let tgtg_messages = Arc::new(RwLock::new(HashMap::new()));

    let dc_data = DiscordData {
        bot_db,
        active_channels: active_channels.clone(),
        tgtg_bindings: tgtg_bindings.clone(),
        tgtg_configs: tgtg_configs.clone(),
        tgtg_messages: tgtg_messages.clone(),
    };

    let mut client = DiscordClient::new(&discord_token, intents, dc_data).await?;

    let http = client.serenity_client.http.clone();

    tokio::spawn(async move {
        // wait 10 secs first to let the bot connect to discord
        tokio::time::sleep(Duration::from_secs(10)).await;
        for (channel_id, _config) in tgtg_configs.read().await.iter() {
            if active_channels.read().await.contains(channel_id) {
                crate::monitor::monitor_location(
                    http.clone(),
                    channel_id.to_owned(),
                    active_channels.clone(),
                    tgtg_bindings.clone(),
                    tgtg_configs.clone(),
                    tgtg_messages.clone(),
                )
                .await;
                info!("Channel {}: Monitor starting (DB) ", channel_id)
            }
        }
    });

    if let Err(why) = client.serenity_client.start().await {
        error!("Client error: {:?}", why);
    }
    Ok(())
}
