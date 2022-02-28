use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use tracing::info;

use crate::{
    Coordinates, ShardManagerContainer, TGTGActiveChannelsContainer, TGTGLocationContainer,
};

static ZOOM_LEVEL: u8 = 15;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Ping responded for channel {}", msg.channel_id);
    msg.channel_id.say(&ctx.http, "Pong!").await?;
    Ok(())
}

#[command]
#[num_args(2)]
async fn set(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        let latitude = args.single::<f64>()?;
        let longitude = args.single::<f64>()?;
        location_map.write().await.insert(
            msg.channel_id,
            Coordinates {
                latitude,
                longitude,
            },
        );
        info!(
            "Location set as ({}, {}) for channel {}",
            latitude, longitude, msg.channel_id
        );
        msg.channel_id
            .send_message(&ctx.http, |m| {
                m.embed(|e| {
                    e.title("Location");
                    e.description("TooGoodToGo location set");
                    e.field("Latitude", format!("{:.4}", latitude), true);
                    e.field("Longitude", format!("{:.4}", longitude), true);
                    e.url(format!(
                        "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                        ZOOM_LEVEL, latitude, longitude
                    ));
                    e
                });
                m
            })
            .await?;
    } else {
        msg.reply(ctx, "There was a problem registering the location")
            .await?;
        return Ok(());
    }
    Ok(())
}

#[command]
async fn start(ctx: &Context, msg: &Message) -> CommandResult {
    let coords = {
        let data = ctx.data.read().await;
        if let Some(location_map) = data.get::<TGTGLocationContainer>() {
            let location_map = location_map.read().await;
            location_map.get(&msg.channel_id).copied()
        } else {
            msg.reply(ctx, "There was a problem with starting monitoring")
                .await?;
            None
        }
    };
    let insert_success = {
        let data = ctx.data.write().await;
        if let Some(active_channels) = data.get::<TGTGActiveChannelsContainer>() {
            let mut active_channels = active_channels.write().await;
            active_channels.insert(msg.channel_id);
            true
        } else {
            msg.reply(ctx, "There was a problem with starting monitoring")
                .await?;
            false
        }
    };
    if let Some(coords) = coords.filter(|_| insert_success) {
        info!("Monitor started for channel {}", msg.channel_id);
        crate::monitor::monitor_location(
            ctx.data.clone(),
            ctx.http.clone(),
            msg.channel_id,
            coords,
        )
        .await;
        msg.react(ctx, '✅').await?;
    }
    Ok(())
}

#[command]
async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(active_channels) = data.get::<TGTGActiveChannelsContainer>() {
        info!("Monitor stopped for channel {}", msg.channel_id);
        let mut active_channels = active_channels.write().await;
        active_channels.remove(&msg.channel_id);
        msg.react(ctx, '✅').await?;
    } else {
        msg.reply(ctx, "There was a problem with stopping monitoring")
            .await?;
    }
    Ok(())
}

#[command]
#[owners_only]
async fn quit(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;
    if let Some(manager) = data.get::<ShardManagerContainer>() {
        msg.reply(ctx, "Shutting down!").await?;
        manager.lock().await.shutdown_all().await;
    } else {
        msg.reply(ctx, "There was a problem getting the shard manager")
            .await?;
        return Ok(());
    }

    Ok(())
}
