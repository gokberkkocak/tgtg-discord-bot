//! Requires the 'framework' feature flag be enabled in your project's
//! `Cargo.toml`.
//!
//! This can be enabled by specifying the feature in the dependency section:
//!
//! ```toml
//! [dependencies.serenity]
//! git = "https://github.com/serenity-rs/serenity.git"
//! features = ["framework", "standard_framework"]
//! ```
mod commands;
mod tgtg;

use std::{
    collections::{HashMap, HashSet},
    env,
    sync::Arc,
    time::Duration,
};

use commands::*;
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::{standard::macros::group, StandardFramework},
    http::Http,
    model::{event::ResumedEvent, gateway::Ready, id::ChannelId},
    prelude::*,
};
use tracing::{error, info};

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

pub struct TGTGLocationContainer;

impl TypeMapKey for TGTGLocationContainer {
    type Value = Arc<RwLock<HashMap<ChannelId, Coordinates>>>;
}

pub struct Coordinates {
    latitude: f64,
    longitude: f64,
}

pub struct TGTGCredentials {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[group]
#[commands(ping, register, quit)]
struct General;

#[tokio::main]
async fn main() {
    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    dotenv::dotenv().expect("Failed to load .env file");

    // Initialize the logger to use environment variables.
    //
    // In this case, a good default is setting the environment variable
    // `RUST_LOG` to `debug`.
    tracing_subscriber::fmt::init();

    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let tgtg_access_token =
        env::var("TGTG_ACCESS_TOKEN").expect("Expected a token in the environment");
    let tgtg_refresh_token =
        env::var("TGTG_REFRESH_TOKEN").expect("Expected a token in the environment");
    let tgtg_user_token = env::var("TGTG_USER_ID").expect("Expected a token in the environment");
    let tgtg_credentials = TGTGCredentials {
        access_token: tgtg_access_token,
        refresh_token: tgtg_refresh_token,
        user_id: tgtg_user_token,
    };

    let http = Http::new_with_token(&discord_token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create the framework
    let framework = StandardFramework::new()
        .configure(|c| c.owners(owners).prefix("!"))
        .group(&GENERAL_GROUP);

    let mut client = Client::builder(&discord_token)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
        data.insert::<TGTGLocationContainer>(Arc::new(RwLock::new(HashMap::new())));
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    tgtg::test_python().expect("python environment error");

    // Loop inform all channels
    let client_data = client.data.clone();
    let http = client.cache_and_http.http.clone();
    tokio::spawn(async move {
        loop {
            let client_data_rw = client_data.read().await;
            let location_map = client_data_rw
                .get::<TGTGLocationContainer>()
                .expect("Could not get Location Map")
                .read()
                .await;
            for (channel, _coords) in location_map.iter() {
                channel
                    .say(&http, "I waited 10 secs and I'm still here!")
                    .await
                    .expect("Could not send message");
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
