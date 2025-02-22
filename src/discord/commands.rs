use anyhow::Context as _;
use poise::serenity_prelude::{self as serenity};

use regex::Regex;
use serenity::all::{CreateEmbed, CreateMessage};
use tracing::info;

use crate::data::{TGTGConfig, OSM_ZOOM_LEVEL, RADIUS_UNIT};

use super::{Context, Error};

/// Check the bot if it's ready to work
#[poise::command[prefix_command, slash_command]]
pub async fn health(ctx: Context<'_>) -> Result<(), Error> {
    info!("Channel {}: Health check recieved.", ctx.channel_id());
    ctx.say("I'm alive and healthy!").await?;
    Ok(())
}

#[poise::command(
    prefix_command,
    slash_command,
    subcommands("default", "radius", "full")
)]
pub async fn location(ctx: Context<'_>) -> Result<(), Error> {
    ctx.say("Hello there!").await?;
    Ok(())
}

/// Sets the location for the bot with default radius (1 km) without filtering
#[poise::command[prefix_command, slash_command]]
pub async fn default(
    ctx: Context<'_>,
    #[description = "latitude"] latitude: f64,
    #[description = "longitude"] longitude: f64,
) -> Result<(), Error> {
    let location_map = &ctx.data().tgtg_configs;
    let location = {
        let exists = location_map.read().await.contains_key(&ctx.channel_id());
        if exists {
            let mut lock = location_map.write().await;
            let location = lock.get_mut(&ctx.channel_id()).context("exists failure")?;
            location.latitude = latitude;
            location.longitude = longitude;
            location.clone()
        } else {
            let location = TGTGConfig::new(latitude, longitude);
            location_map
                .write()
                .await
                .insert(ctx.channel_id(), location.clone());
            location
        }
    };

    let bot_db = &ctx.data().bot_db;
    bot_db.set_location(ctx.channel_id(), &location).await?;
    info!(
        "Channel {}: Location set ({}, {})",
        ctx.channel_id(),
        latitude,
        longitude,
    );
    let mut embed = CreateEmbed::new()
        .title("Location")
        .description("TooGoodToGo location is set for this channel")
        .url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, latitude, longitude
        ))
        .field("Latitude", format!("{:.4}", latitude), true)
        .field("Longitude", format!("{:.4}", longitude), true)
        .field(
            "Radius",
            format!("{} {}", location.radius, RADIUS_UNIT),
            true,
        );
    if let Some(regex) = &location.regex {
        embed = embed.field("Regex", regex.as_str().replace('*', "\\*"), true);
    }
    ctx.reply("Location has been set!").await?;
    ctx.channel_id()
        .send_message(ctx.http(), CreateMessage::new().add_embed(embed))
        .await?;
    Ok(())
}

/// Sets the location for the bot with radius info without filtering
#[poise::command[prefix_command, slash_command]]
async fn radius(
    ctx: Context<'_>,
    #[description = "latitude"] latitude: f64,
    #[description = "longitude"] longitude: f64,
    #[description = "radius"] radius: u8,
) -> Result<(), Error> {
    let location_map = &ctx.data().tgtg_configs;
    let location = {
        let exists = location_map.read().await.contains_key(&ctx.channel_id());
        if exists {
            let mut lock = location_map.write().await;
            let location = lock.get_mut(&ctx.channel_id()).context("exists failure")?;
            location.latitude = latitude;
            location.longitude = longitude;
            location.radius = radius;
            location.clone()
        } else {
            let location = TGTGConfig::new_with_radius(latitude, longitude, radius);
            location_map
                .write()
                .await
                .insert(ctx.channel_id(), location.clone());
            location
        }
    };

    let bot_db = &ctx.data().bot_db;
    bot_db.set_location(ctx.channel_id(), &location).await?;
    info!("Channel {}: Radius set {} ", ctx.channel_id(), radius);
    let mut embed = CreateEmbed::new()
        .title("Radius")
        .description("TooGoodToGo radius is set for this channel")
        .field("Latitude", format!("{:.4}", location.latitude), true)
        .field("Longitude", format!("{:.4}", location.longitude), true)
        .field("Radius", format!("{} {}", radius, RADIUS_UNIT), true)
        .url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, location.latitude, location.longitude
        ));
    if let Some(regex) = &location.regex {
        embed = embed.field("Regex", regex.as_str().replace('*', "\\*"), true);
    }
    ctx.reply("Location has been set!").await?;
    ctx.channel_id()
        .send_message(&ctx.http(), CreateMessage::new().add_embed(embed))
        .await?;

    Ok(())
}

/// Sets the location with given radius with regex filter
#[poise::command[prefix_command, slash_command]]
async fn full(
    ctx: Context<'_>,
    #[description = "latitude"] latitude: f64,
    #[description = "longitude"] longitude: f64,
    #[description = "radius"] radius: u8,
    #[description = "regex"] regex: String,
) -> Result<(), Error> {
    let location_map = &ctx.data().tgtg_configs;
    let location = {
        let exists = location_map.read().await.contains_key(&ctx.channel_id());
        if exists {
            let mut lock = location_map.write().await;
            let location = lock.get_mut(&ctx.channel_id()).context("exists failure")?;
            location.latitude = latitude;
            location.longitude = longitude;
            location.radius = radius;
            location.regex = Some(Regex::new(&regex)?);
            location.clone()
        } else {
            let regex = Regex::new(&regex)?;
            let location = TGTGConfig::new_full(latitude, longitude, radius, regex);
            location_map
                .write()
                .await
                .insert(ctx.channel_id(), location.clone());
            location
        }
    };

    let bot_db = &ctx.data().bot_db;
    bot_db.set_location(ctx.channel_id(), &location).await?;
    info!("Channel {}: Radius set {} ", ctx.channel_id(), radius);
    let mut embed = CreateEmbed::new()
        .title("Radius")
        .description("TooGoodToGo radius is set for this channel")
        .field("Latitude", format!("{:.4}", location.latitude), true)
        .field("Longitude", format!("{:.4}", location.longitude), true)
        .field("Radius", format!("{} {}", radius, RADIUS_UNIT), true)
        .url(format!(
            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
            OSM_ZOOM_LEVEL, location.latitude, location.longitude
        ));
    if let Some(regex) = &location.regex {
        embed = embed.field("Regex", regex.as_str().replace('*', "\\*"), true);
    }
    ctx.reply("Location has been set!").await?;
    ctx.channel_id()
        .send_message(&ctx.http(), CreateMessage::new().add_embed(embed))
        .await?;

    Ok(())
}

/// Check the status for the current channel
#[poise::command[prefix_command, slash_command]]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let location_map = &ctx.data().tgtg_configs;
    match location_map.read().await.get(&ctx.channel_id()) { Some(location) => {
        let active_channels = &ctx.data().active_channels;
        let is_active = active_channels
            .read()
            .await
            .iter()
            .any(|c| c.channel_id == ctx.channel_id());
        let mut embed = CreateEmbed::new()
            .title("Monitor Status")
            .description("TooGoodToGo monitor status")
            .field("Latitude", format!("{:.4}", location.latitude), true)
            .field("Longitude", format!("{:.4}", location.longitude), true)
            .field(
                "Radius",
                format!("{} {}", location.radius, RADIUS_UNIT),
                true,
            )
            .url(format!(
                "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                OSM_ZOOM_LEVEL, location.latitude, location.longitude
            ));
        if let Some(regex) = &location.regex {
            embed = embed.field("Regex", regex.as_str().replace('*', "\\*"), true);
        }
        embed = embed.field("Active", if is_active { "✅" } else { "❌" }, true);
        let message = CreateMessage::new().add_embed(embed);
        ctx.channel_id().send_message(&ctx.http(), message).await?;
        ctx.reply("Here's the status!").await?;
    } _ => {
        ctx.reply("Location is not found!").await?;
    }}
    Ok(())
}

/// Start monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    // Are we already monitoring
    if ctx
        .data()
        .active_channels
        .read()
        .await
        .iter()
        .map(|a| a.channel_id)
        .any(|a| ctx.channel_id() == a)
    {
        ctx.reply("Already monitoring!").await?;
        return Ok(());
    }
    match ctx.data().tgtg_configs.read().await.get(&ctx.channel_id()) { Some(tgtg_config) => {
        let active_channels = &ctx.data().active_channels;
        let bot_db = &ctx.data().bot_db;
        bot_db.change_active(ctx.channel_id(), true).await?;
        let http = ctx.serenity_context().http.clone();
        let cm = crate::monitor::ChannelMonitor::init(
            http,
            ctx.channel_id(),
            ctx.data().tgtg_bindings.clone(),
            tgtg_config.clone(),
        );

        let mut active_channels = active_channels.write().await;
        active_channels.insert(cm);
        info!("Channel {}: Monitor starting", ctx.channel_id());
        ctx.reply("Started monitoring!").await?;
    } _ => {
        info!("Channel {}: Could not start Monitor", ctx.channel_id());
        ctx.reply("Location is not found!").await?;
    }}

    Ok(())
}

/// Start monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    // Are we already monitoring
    if ctx
        .data()
        .active_channels
        .read()
        .await
        .iter()
        .map(|a| a.channel_id)
        .all(|a| ctx.channel_id() != a)
    {
        ctx.reply("There is nothing to stop!").await?;
        return Ok(());
    }

    let active_channels = &ctx.data().active_channels;
    let mut active_channels = active_channels.write().await;
    active_channels.retain(|c| c.channel_id != ctx.channel_id());

    let bot_db = &ctx.data().bot_db;
    bot_db.change_active(ctx.channel_id(), false).await?;

    ctx.reply("Stopped monitoring!").await?;
    info!("Channel {}: Monitor stopping", ctx.channel_id());

    Ok(())
}
