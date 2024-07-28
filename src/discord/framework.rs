use poise::serenity_prelude as serenity;

use serenity::all::GatewayIntents;
use serenity::Client;

use crate::data::DiscordData;

pub struct DiscordClient {
    pub serenity_client: Client,
}

impl DiscordClient {
    pub async fn new(
        token: &str,
        intents: GatewayIntents,
        data: DiscordData,
    ) -> anyhow::Result<Self> {
        let framework = poise::Framework::builder()
            .options(poise::FrameworkOptions {
                commands: vec![
                    super::commands::health(),
                    super::commands::location(),
                    super::commands::status(),
                    super::commands::start(),
                    super::commands::stop(),
                ],
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

        Ok(DiscordClient {
            serenity_client: client?,
        })
    }
}
