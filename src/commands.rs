use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;
use tracing::info;

use crate::{Coordinates, ShardManagerContainer, TGTGLocationContainer};

static OPENSTREETMAP_LINK_TEMPLATE: &str = "https://www.openstreetmap.org/#map={}/{}/{}";
static ZOOM_LEVEL: u8 = 15;

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    info!("Ping responded for channel {}", msg.channel_id);
    msg.channel_id.say(&ctx.http, "Pong!").await?;
    Ok(())
}

#[command]
#[num_args(2)]
async fn monitor(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.read().await;
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
            "Monitor started ({}, {}) for channel {}",
            latitude, longitude, msg.channel_id
        );
        msg.channel_id..send_message(&http, |m| {
            m.embed(|e| {
                e.title("Monitoring started");
                e.field("Latitude", format!("{:.4}", latitude), true);
                e.field("Longitude", format!("{:.4}", longitude), true);
                e.url(format!(
                    OPENSTREETMAP_LINK_TEMPLATE,
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
