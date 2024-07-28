use poise::serenity_prelude as serenity;
use ::serenity::all::GatewayIntents;
use serenity::Client;

use crate::DiscordData;

pub struct DiscordClient {
    pub client: Client
}

impl DiscordClient {
    pub async fn new(token: &str, intents: GatewayIntents, data: DiscordData) -> anyhow::Result<Self>{
        let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![super::commands::addmultiple()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(data)
            })
        })
        .build();

        let client = serenity::ClientBuilder::new(token, intents)
            .framework(framework)
            .await;
    
        Ok(DiscordClient { client: client? })   
    
    }
}
