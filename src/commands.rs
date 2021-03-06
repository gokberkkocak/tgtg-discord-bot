use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use tracing::info;

use crate::{
    BotDBContainer, CoordinatesWithRadius, ShardManagerContainer, TGTGActiveChannelsContainer,
    TGTGLocationContainer, RADIUS_UNIT,
};

static OSM_ZOOM_LEVEL: u8 = 15;
static DEFAULT_RADIUS: u8 = 3;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Channel {}: Ping recieved.", msg.channel_id);
    msg.channel_id.say(&ctx.http, "Pong!").await?;
    Ok(())
}

#[command]
#[num_args(2)]
async fn location(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        let latitude = args.single::<f64>()?;
        let longitude = args.single::<f64>()?;
        let coords = CoordinatesWithRadius {
            latitude,
            longitude,
            radius: DEFAULT_RADIUS,
        };
        location_map.write().await.insert(msg.channel_id, coords);
        info!(
            "Channel {}: Location set ({}, {})",
            msg.channel_id, latitude, longitude,
        );
        msg.channel_id
            .send_message(&ctx.http, |m| {
                m.embed(|e| {
                    e.title("Location");
                    e.description("TooGoodToGo location is set for this channel");
                    e.field("Latitude", format!("{:.4}", latitude), true);
                    e.field("Longitude", format!("{:.4}", longitude), true);
                    e.field(
                        "Radius",
                        format!("{} {}", DEFAULT_RADIUS, RADIUS_UNIT),
                        true,
                    );
                    e.url(format!(
                        "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                        OSM_ZOOM_LEVEL, latitude, longitude
                    ));
                    e
                });
                m
            })
            .await?;
        if let Some(bot_db) = data.get::<BotDBContainer>() {
            bot_db.set_location(msg.channel_id, coords).await?;
        }
    } else {
        msg.reply(ctx, "There was a problem registering the location")
            .await?;
    }
    Ok(())
}

#[command]
#[num_args(1)]
async fn radius(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        let radius = args.single::<u8>()?;
        if let Some(location) = location_map.write().await.get_mut(&msg.channel_id) {
            location.radius = radius;
            if let Some(bot_db) = data.get::<BotDBContainer>() {
                let new_coords = CoordinatesWithRadius {
                    latitude: location.latitude,
                    longitude: location.longitude,
                    radius,
                };
                bot_db.set_location(msg.channel_id, new_coords).await?;
                info!("Channel {}: Radius set {} ", msg.channel_id, radius);
                msg.channel_id
                    .send_message(&ctx.http, |m| {
                        m.embed(|e| {
                            e.title("Radius");
                            e.description("TooGoodToGo radius is set for this channel");
                            e.field("Latitude", format!("{:.4}", location.latitude), true);
                            e.field("Longitude", format!("{:.4}", location.longitude), true);
                            e.field("Radius", format!("{} {}", radius, RADIUS_UNIT), true);
                            e.url(format!(
                                "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                                OSM_ZOOM_LEVEL, location.latitude, location.longitude
                            ));
                            e
                        });
                        m
                    })
                    .await?;
            } else {
                msg.reply(ctx, "There was a problem registering the location")
                    .await?;
            }
        } else {
            msg.reply(
                ctx,
                "There was a problem registering the radius (retriving the location)",
            )
            .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the radius (retrieving the client data)",
        )
        .await?;
    }
    Ok(())
}

#[command]
async fn status(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.read().await;
    let is_active = {
        if let Some(active_channels) = data.get::<TGTGActiveChannelsContainer>() {
            active_channels.read().await.contains(&msg.channel_id)
        } else {
            msg.reply(
                ctx,
                "There was a problem registering the radius (retriving the active message)",
            )
            .await?;
            false
        }
    };
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        if let Some(location) = location_map.read().await.get(&msg.channel_id) {
            msg.channel_id
                .send_message(&ctx.http, |m| {
                    m.embed(|e| {
                        e.title("Monitor Status");
                        e.description("TooGoodToGo monitor status");
                        e.field("Latitude", format!("{:.4}", location.latitude), true);
                        e.field("Longitude", format!("{:.4}", location.longitude), true);
                        e.field(
                            "Radius",
                            format!("{} {}", location.radius, RADIUS_UNIT),
                            true,
                        );
                        e.field("Active", if is_active { "???" } else { "???" }, true);
                        e.url(format!(
                            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                            OSM_ZOOM_LEVEL, location.latitude, location.longitude
                        ));
                        e
                    });
                    m
                })
                .await?;
        } else {
            msg.reply(
                ctx,
                "There was a problem registering the radius (retriving the location)",
            )
            .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the radius (retrieving the client data)",
        )
        .await?;
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
            if let Some(bot_db) = data.get::<BotDBContainer>() {
                bot_db.change_active(msg.channel_id, true).await?;
                true
            } else {
                msg.reply(ctx, "There was a problem with starting monitoring (db)")
                    .await?;
                false
            }
        } else {
            msg.reply(
                ctx,
                "There was a problem with starting monitoring (active channel set)",
            )
            .await?;
            false
        }
    };
    if let Some(coords) = coords.filter(|_| insert_success) {
        crate::monitor::monitor_location(
            ctx.data.clone(),
            ctx.http.clone(),
            msg.channel_id,
            coords,
        )
        .await;
        msg.react(ctx, '????').await?;
        info!("Channel {}: Monitor starting", msg.channel_id);
    }
    Ok(())
}

#[command]
async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(active_channels) = data.get::<TGTGActiveChannelsContainer>() {
        let mut active_channels = active_channels.write().await;
        active_channels.remove(&msg.channel_id);
        if let Some(bot_db) = data.get::<BotDBContainer>() {
            bot_db.change_active(msg.channel_id, false).await?;
            msg.react(ctx, '????').await?;
            info!("Channel {}: Monitor stopping", msg.channel_id);
        } else {
            msg.reply(ctx, "There was a problem with starting monitoring (db)")
                .await?;
        }
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
    }
    Ok(())
}
