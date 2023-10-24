use twitch_irc::login::StaticLoginCredentials;
use twitch_irc::TwitchIRCClient;
use twitch_irc::{ClientConfig, SecureTCPTransport};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();

    args.next(); //exe name

    let channel_name = args.next()
        .ok_or_else(|| "Channel name is required as first arg")?;

    tracing_subscriber::fmt::init();

    tracing::info!("Attempting to connect to {channel_name}");

    // default configuration is to join chat as anonymous.
    let config = ClientConfig::default();
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    // first thing you should do: start consuming incoming messages,
    // otherwise they will back up.
    let join_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            tracing::info!("Received message: {:?}", message);
        }
    });

    client.join(channel_name)?;

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await?;

    Ok(())
}
