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

                match server_message {
                    Ping(_) | Pong(_) => {
                        tracing::debug!("Received: {server_message:?}");
                    }
                    _ => tracing::info!("Received: {server_message:?}"),
                }

                match server_message {
                    Privmsg(message) => {
                        tracing::info!("Received Privmsg");

                        if let Some(response) = ardly_bot::response(
                            &message.message_text
                        ) {
                            let reply_result = client.say_in_reply_to(
                                &message,
                                format!("'ardly-bot sez: {}", &response),
                            ).await;
    
                            if let Err(err) = reply_result {
                                tracing::error!("say_in_reply_to error: {err}");
                            } else {
                                tracing::info!("Replied with \"{response}\"!");
                            }
                        }
                    }
                    Ping(_) | Pong(_) => {}
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

mod ardly_bot {
    use regex::Regex;

    pub fn response(input: &str) -> Option<String> {
        // avoid self replies
        if input.contains("know 'er") {
            return None;
        }

        // TODO make this a lazy static.
        let er_regex = Regex::new(r"(\P{White_Space}+)er(s|ed|ing)?(\p{White_Space}|[\.!?,]|$)").unwrap();

        let mut best_word = "";
        for captures in er_regex.captures_iter(input) {
            // 0 is the whole match
            let Some(mut erless_word) = captures.get(1).map(|c| c.as_str()) else {
                continue
            };

            // This seems easier than changing the regex to handle "ererer"
            while erless_word.ends_with("er") {
                erless_word = &erless_word[0..erless_word.len() - 2];
            }

            // TODO? Better bestness criteria?
            if best_word.len() <= erless_word.len() {
                best_word = erless_word;
            }
        }

        if best_word.is_empty() {
            return None;
        }

        Some(format!(
            "{best_word} 'er? I 'ardly know 'er!"
        ))
    }
}