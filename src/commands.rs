use serenity::framework::standard::Args;
use serenity::framework::standard::{macros::command, CommandResult};
use serenity::model::prelude::*;
use serenity::prelude::*;

use crate::{ShardManagerContainer, TGTGLocationContainer, Coordinates};


#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    msg.channel_id.say(&ctx.http, "Pong!").await?;
    Ok(())
}

#[command]
#[num_args(2)]
async fn register(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.read().await;
    if let Some(location_map) = data.get::<TGTGLocationContainer>() {
        let latitude = args.single::<f64>()?;
        let longitude = args.single::<f64>()?;
        location_map.write().await.insert(msg.channel_id, Coordinates {
            latitude,
            longitude,
        });
        msg.channel_id.say(&ctx.http, format!("Registered to this channel with ({latitude}, {longitude})!")).await?;
    } else {
        msg.reply(ctx, "There was a problem registering the location").await?;
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
        msg.reply(ctx, "There was a problem getting the shard manager").await?;

        return Ok(());
    }

    Ok(())
}