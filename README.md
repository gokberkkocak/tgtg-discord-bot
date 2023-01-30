# TooGoodToGo Discord Bot 🥪

This discord bot helps you keep track what TGTG magic bags available in a given area. 

When the bot is set, it posts available bags to a discord channel. It's capable to update and delete them when the quantity changes.

## Requirements

Use the release binaries or check the compile section at the bottom. 

You will need a Discord API Token and TGTG credentials. 
- For Discord, you can go [here](https://discord.com/developers/applications) and create a bot retrieve its token.
- For TGTG, the system uses unofficial TGTG api from [here](https://github.com/ahivert/tgtg-python). Check retrieve tokens section and retrieve your access token, refresh token and user id.

Once you have the necessary token, put them into your environment variables (or .env file). 

The system also uses an sqlite db system for the bot to remember channels and locations from previous runs. 

You can generate an empty sqlite db as follows:

```
sqlite3 bot.db < migrations/20220301134633_bot.sql 
```

You should also set the db environment variable (DATABASE_URL) as well.

Example ```.env``` file for environment variables:
```
TGTG_ACCESS_TOKEN=XXX
TGTG_REFRESH_TOKEN=XXX
TGTG_USER_ID=XXX
TGTG_COOKIE=XXX
DISCORD_TOKEN=XXX
DATABASE_URL=sqlite:bot.db
RUST_LOG=info
```

Install python dependencies to your python environment with:

```
pip install -r requirements.txt
```

## Bot permissions

Invite your created bot to your discord server with the following permissions.

### Scopes:
```
bot
applications.commands
```

### Bot permissions:
```
Send messages
Manage messages
Embed links
Add reactions
```

### Privileged Gateway Intents

```
Message Content Intent
```

## Bot Usage

Available bot commands:
```
tg!location <latitude> <longitude>
tg!radius <radius in km>
tg!regex <regular expression to filter bags>
tg!start
tg!stop
tg!status
tg!quit
```

You should register the location as the first command to be able use the bot. You can retrieve the wanted location's latitude and longitude on OpenStreetMaps' address bar or on Google Maps' context menu. Setting a radius is optional. It defaults to 3 km. 

### Example - Setting a location

![Location](images/location.png)

The bot responds with a message to confirm.

### Example - Starting

![Start](images/start.png)

The bot acknowledges the command with a reaction. This applies to stop as well.

### Example - Status

![Start](images/status.png)

The bot responds with the confirmation of the  location and its monitoring status.

### Example - Listing

![Listing](images/listing.png)

The listing includes price, quantity and distance. The bot automatically updates if the quantity changes or deletes it if the item is not available anymore.

## Compilation

The system uses sqlx for compile time query verification. Therefore, the database file needs to be present and loaded into the environment variable at compile time.

```
export DATABASE_URL=sqlite:bot.db
sqlite3 bot.db < migrations/20220301134633_bot.sql 
pip install -r requirements.txt
cargo b --release
```

## Why Rust-Python Bridge

While the unofficial tgtg API is only available on Python, there are plenty of discord API targetting libraries in various languages. The main reason this project uses Rust on top of tgtg python api is that I wanted to try out ```pyo3``` framework which bridges python land with rust. I found the framework very flexible. The second reason is to try a discord library in rust language. For this purpose, I used the ```serenity``` crate for discord which seems powerful. 
