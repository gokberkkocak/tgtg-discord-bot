use regex::Regex;
use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use tracing::info;

use crate::{
    BotDBContainer, TGTGLocation, ShardManagerContainer, TGTGActiveChannelsContainer,
    TGTGLocationContainer, OSM_ZOOM_LEVEL, RADIUS_UNIT,
};

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
        if let Some(bot_db) = data.get::<BotDBContainer>() {
            let latitude = args.single::<f64>()?;
            let longitude = args.single::<f64>()?;
            let location = {
                if let Some(location) = location_map.write().await.get_mut(&msg.channel_id) {
                    location.latitude = latitude;
                    location.longitude = longitude;
                    location.clone()
                } else {
                    let location = TGTGLocation::new(latitude, longitude);
                    location_map
                        .write()
                        .await
                        .insert(msg.channel_id, location.clone());
                    location
                }
            };
            bot_db.set_location(msg.channel_id, &location).await?;
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
                            format!("{} {}", location.radius, RADIUS_UNIT),
                            true,
                        );
                        if let Some(regex) = &location.regex {
                            e.field("Regex", regex.as_str().replace("*", "\\*"), true);
                        }
                        e.url(format!(
                            "https://www.openstreetmap.org/#map={}/{:.4}/{:.4}",
                            OSM_ZOOM_LEVEL, latitude, longitude
                        ));
                        e
                    });
                    m
                })
                .await?;
        } else {
            msg.channel_id
                .say(
                    ctx,
                    "There was a problem registering the location (database)",
                )
                .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the location (client data)",
        )
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
                bot_db.set_location(msg.channel_id, &location).await?;
                info!("Channel {}: Radius set {} ", msg.channel_id, radius);
                msg.channel_id
                    .send_message(&ctx.http, |m| {
                        m.embed(|e| {
                            e.title("Radius");
                            e.description("TooGoodToGo radius is set for this channel");
                            e.field("Latitude", format!("{:.4}", location.latitude), true);
                            e.field("Longitude", format!("{:.4}", location.longitude), true);
                            e.field("Radius", format!("{} {}", radius, RADIUS_UNIT), true);
                            if let Some(regex) = &location.regex {
                                e.field("Regex", regex.as_str().replace("*", "\\*"), true);
                            }
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
                msg.reply(ctx, "There was a problem registering the radius (database)")
                    .await?;
            }
        } else {
            msg.reply(ctx, "There was a problem registering the radius (location)")
                .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the radius (client data)",
        )
        .await?;
    }
    Ok(())
}

#[command]
#[num_args(1)]
async fn regex(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.write().await;
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        let regex_string = args.single::<String>()?;
        if let Some(location) = location_map.write().await.get_mut(&msg.channel_id) {
            if let Some(bot_db) = data.get::<BotDBContainer>() {
                if let Ok(regex) = Regex::new(&regex_string) {
                    location.regex = Some(regex);
                    bot_db.set_location(msg.channel_id, &location).await?;
                    info!("Channel {}: Regex set {}", msg.channel_id, regex_string);
                    msg.channel_id
                        .send_message(&ctx.http, |m| {
                            m.embed(|e| {
                                e.title("Regex");
                                e.description("TooGoodToGo regex is set for this channel");
                                e.field("Latitude", format!("{:.4}", location.latitude), true);
                                e.field("Longitude", format!("{:.4}", location.longitude), true);
                                e.field(
                                    "Radius",
                                    format!("{} {}", location.radius, RADIUS_UNIT),
                                    true,
                                );
                                e.field("Regex", regex_string.replace("*", "\\*"), true);
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
                    msg.reply(ctx, "The regex is not valid").await?;
                }
            } else {
                msg.reply(ctx, "There was a problem registering the regex (database)")
                    .await?;
            }
        } else {
            msg.reply(ctx, "There was a problem registering the regex (location)")
                .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the regex (client data)",
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
                "There was a problem registering the radius (active channel set)",
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
                        if let Some(regex) = &location.regex {
                            e.field("Regex", regex.as_str().replace("*", "\\*"), true);
                        }
                        e.field("Active", if is_active { "✅" } else { "❌" }, true);
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
            msg.reply(ctx, "There was a problem registering the radius (location)")
                .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem registering the radius (client data)",
        )
        .await?;
    }
    Ok(())
}

#[command]
async fn start(ctx: &Context, msg: &Message) -> CommandResult {
    let insert_success = {
        let data = ctx.data.write().await;
        if let Some(active_channels) = data.get::<TGTGActiveChannelsContainer>() {
            let mut active_channels = active_channels.write().await;
            active_channels.insert(msg.channel_id);
            if let Some(bot_db) = data.get::<BotDBContainer>() {
                bot_db.change_active(msg.channel_id, true).await?;
                true
            } else {
                msg.reply(
                    ctx,
                    "There was a problem with starting monitoring (database)",
                )
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
    if insert_success {
        crate::monitor::monitor_location(ctx.data.clone(), ctx.http.clone(), msg.channel_id).await;
        msg.react(ctx, '👍').await?;
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
            msg.react(ctx, '👍').await?;
            info!("Channel {}: Monitor stopping", msg.channel_id);
        } else {
            msg.reply(
                ctx,
                "There was a problem with stopping monitoring (database)",
            )
            .await?;
        }
    } else {
        msg.reply(
            ctx,
            "There was a problem with stopping monitoring (active channel set)",
        )
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
        msg.reply(ctx, "There was a problem with quitting (shard manager)")
            .await?;
    }
    Ok(())
}
