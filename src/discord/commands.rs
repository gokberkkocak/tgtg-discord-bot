use poise::serenity_prelude as serenity;

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
    let bot_db = &ctx.data().bot_db;
    let location = {
        if let Some(location) = location_map.write().await.get_mut(&ctx.channel_id()) {
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
    let bot_db = &ctx.data().bot_db;
    let location = {
        if let Some(location) = location_map.write().await.get_mut(&ctx.channel_id()) {
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
    let bot_db = &ctx.data().bot_db;
    let location = {
        if let Some(location) = location_map.write().await.get_mut(&ctx.channel_id()) {
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
    let active_channels = &ctx.data().active_channels;
    let is_active = active_channels.read().await.contains(&ctx.channel_id());

    let location_map = &ctx.data().tgtg_configs;

    if let Some(location) = location_map.read().await.get(&ctx.channel_id()) {
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
    } else {
        ctx.reply("There was a problem registering the radius (location)")
            .await?;
    }
    ctx.reply("Here's the status!").await?;
    Ok(())
}

/// Start monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
    let active_channels = &ctx.data().active_channels;
    let bot_db = &ctx.data().bot_db;
    let mut active_channels = active_channels.write().await;
    active_channels.insert(ctx.channel_id());
    bot_db.change_active(ctx.channel_id(), true).await?;

    let http = ctx.serenity_context().http.clone();
    crate::monitor::monitor_location(
        http,
        ctx.channel_id(),
        ctx.data().active_channels.clone(),
        ctx.data().tgtg_bindings.clone(),
        ctx.data().tgtg_configs.clone(),
        ctx.data().tgtg_messages.clone(),
    )
    .await;
    ctx.reply("Starting now!").await?;
    info!("Channel {}: Monitor starting", ctx.channel_id());

    Ok(())
}

/// Start monitoring TGTG for the channel
#[poise::command[prefix_command, slash_command]]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let active_channels = &ctx.data().active_channels;

    let mut active_channels = active_channels.write().await;
    active_channels.remove(&ctx.channel_id());

    let bot_db = &ctx.data().bot_db;

    bot_db.change_active(ctx.channel_id(), false).await?;
    ctx.reply("Stopping now!").await?;
    info!("Channel {}: Monitor stopping", ctx.channel_id());

    Ok(())
}
