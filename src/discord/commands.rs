use poise::serenity_prelude as serenity;

use super::{Context, Error};

/// Adds multiple numbers
///
/// Demonstrates `#[min]` and `#[max]`
#[poise::command(prefix_command, slash_command)]
pub async fn addmultiple(
    ctx: Context<'_>,
    #[description = "An operand"] a: i8,
    #[description = "An operand"] b: u64,
    #[description = "An operand"]
    #[min = 1234567890123456_i64]
    #[max = 1234567890987654_i64]
    c: i64,
) -> Result<(), Error> {
    ctx.say(format!("Result: {}", a as i128 + b as i128 + c as i128))
        .await?;

    Ok(())
}