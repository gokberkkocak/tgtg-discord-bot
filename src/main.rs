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
    model::{
        event::ResumedEvent,
        gateway::Ready,
        id::{ChannelId, MessageId},
    },
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

#[derive(Clone, Copy)]
pub struct ItemMessage {
    pub message_id: MessageId,
    pub quantity: usize,
}

pub struct TGTGItemContainer;

impl TypeMapKey for TGTGItemContainer {
    type Value = Arc<RwLock<HashMap<String, ItemMessage>>>;
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
#[commands(ping, monitor, quit)]
struct General;

#[tokio::main]
async fn main() {
    // Load .env variables if it exists.
    dotenv::dotenv().ok();
    
    // Initialize the logger to use environment variables.
    tracing_subscriber::fmt::init();

    let discord_token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let tgtg_access_token =
        env::var("TGTG_ACCESS_TOKEN").expect("Expected a token in the environment");
    let tgtg_refresh_token =
        env::var("TGTG_REFRESH_TOKEN").expect("Expected a token in the environment");
    let tgtg_user_token = env::var("TGTG_USER_ID").expect("Expected a token in the environment");
    let tgtg_credentials = Arc::new(TGTGCredentials {
        access_token: tgtg_access_token,
        refresh_token: tgtg_refresh_token,
        user_id: tgtg_user_token,
    });

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
        data.insert::<TGTGItemContainer>(Arc::new(RwLock::new(HashMap::new())));
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
    monitor_locations(tgtg_credentials, client_data, http).await;

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}

async fn monitor_locations(
    tgtg_credentials: Arc<TGTGCredentials>,
    client_data: Arc<RwLock<TypeMap>>,
    http: Arc<Http>,
) {
    tokio::spawn(async move {
        loop {
            let client_data_rw = client_data.read().await;
            let location_map = client_data_rw
                .get::<TGTGLocationContainer>()
                .expect("Could not get Location Map")
                .read()
                .await;
            for (channel, coords) in location_map.iter() {
                let items =
                    tgtg::get_items(&tgtg_credentials, coords).expect("Could not get items");
                for i in items {
                    let item_message = {
                        let item_map = client_data_rw
                        .get::<TGTGItemContainer>()
                        .expect("Could not get Item Map")
                        .read()
                        .await;
                        item_map.get(&i.item.item_id).copied()
                    };
                    //  Check if the item is available
                    if i.items_available > 0 {
                        if let Some(item_message) = item_message {
                            // Update the message with the new quantity
                            if item_message.quantity != i.items_available {
                                channel
                                    .edit_message(&http, item_message.message_id, |m| {
                                        m.embed(|e| {
                                            e.title(i.store.store_name);
                                            e.description(i.display_name);
                                            e.field(
                                                "Price",
                                                format!(
                                                    "{:.2} {}",
                                                    i.item.price_including_taxes.minor_units as f64
                                                        / 10u32.pow(
                                                            i.item.price_including_taxes.decimals
                                                        )
                                                            as f64,
                                                    i.item.price_including_taxes.code
                                                ),
                                                true,
                                            );
                                            e.field("Quantity", i.items_available, true);
                                            e.image(i.store.logo_picture.current_url);
                                            e
                                        });
                                        m
                                    })
                                    .await
                                    .expect("Could not edit message");
                                let mut item_map = client_data_rw
                                    .get::<TGTGItemContainer>()
                                    .expect("Could not get Item Map")
                                    .write()
                                    .await;
                                item_map.insert(
                                    i.item.item_id,
                                    ItemMessage {
                                        message_id: item_message.message_id,
                                        quantity: i.items_available,
                                    },
                                );
                            }
                        } else {
                            // We have quantity available, post a new message
                            let msg = channel
                                .send_message(&http, |m| {
                                    m.embed(|e| {
                                        e.title(i.store.store_name);
                                        e.description(i.display_name);
                                        e.field(
                                            "Price",
                                            format!(
                                                "{:.2} {}",
                                                i.item.price_including_taxes.minor_units as f64
                                                    / 10u32
                                                        .pow(i.item.price_including_taxes.decimals)
                                                        as f64,
                                                i.item.price_including_taxes.code
                                            ),
                                            true,
                                        );
                                        e.field("Quantity", i.items_available, true);
                                        e.image(i.store.logo_picture.current_url);
                                        e
                                    });
                                    m
                                })
                                .await
                                .expect("Could not send message");
                            let mut item_map_write = client_data_rw
                                .get::<TGTGItemContainer>()
                                .expect("Could not get Item Map")
                                .write()
                                .await;
                            item_map_write.insert(
                                i.item.item_id,
                                ItemMessage {
                                    message_id: msg.id,
                                    quantity: i.items_available,
                                },
                            );
                        }
                    } else {
                        // No quantity. Check we posted this item before, if yes delete
                        if let Some(item_message) = item_message {
                            channel
                                .delete_message(&http, item_message.message_id)
                                .await
                                .expect("Could not delete message");
                            let mut item_map_write = client_data_rw
                                .get::<TGTGItemContainer>()
                                .expect("Could not get Item Map")
                                .write()
                                .await;
                            item_map_write.remove(&i.item.item_id);
                        }
                    }
                }
            }
            // manually drop locks. we don't want to keep the lock during the sleeping period.
            drop(location_map);
            drop(client_data_rw);
            tokio::time::sleep(Duration::from_secs(45)).await;
        }
    });
}
