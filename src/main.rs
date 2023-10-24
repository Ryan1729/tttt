use twitch_irc::{
    ClientConfig,
    SecureTCPTransport,
    TwitchIRCClient,
    login::StaticLoginCredentials,
    message::ServerMessage,
};

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();

    args.next(); //exe name

    let channel_name = args.next()
        .ok_or_else(|| "Channel name is required as first arg")?;

    let login_name = args.next()
        .ok_or_else(|| "Login name is required as second arg")?;

    let oauth_token = args.next()
        .ok_or_else(|| "OAuth token is required as third arg")?;

    tracing_subscriber::fmt::init();

    tracing::info!("Attempting to connect to {channel_name} as {login_name}");

    // default configuration is to join chat as anonymous.
    let config = ClientConfig::new_simple(
        StaticLoginCredentials::new(login_name, Some(oauth_token))
    );
    let (mut incoming_messages, client) =
        TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(config);

    let join_handle = tokio::spawn({
        let client = client.clone();

        async move {
            // It is important to be consuming incoming messages,
            // otherwise they will back up.
            while let Some(server_message) = incoming_messages.recv().await {
                use ServerMessage::*;
                tracing::info!("Received: {server_message:?}");

                match server_message {
                    Privmsg(message) => {
                        tracing::info!("Received Privmsg");

                        let reply_result = client.say_in_reply_to(
                            &message,
                            "I saw that!".to_owned()
                        ).await;

                        if let Err(err) = reply_result {
                            tracing::error!("say_in_reply_to error: {err}");
                        } else {
                            tracing::info!("Replied!");
                        }
                    }
                    _ => {
                        tracing::info!("Unhandled mesage type");
                    }
                }
            }
        }
    });

    client.join(channel_name)?;

    // keep the tokio executor alive.
    // If you return instead of waiting the background task will exit.
    join_handle.await?;

    Ok(())
}
