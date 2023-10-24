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
                            message.message_text.clone()
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

    pub fn response(mut input: String) -> Option<String> {
        input.make_ascii_lowercase();

        // avoid self replies
        if input.contains("know 'er") {
            return None;
        }

        if input.contains("liquor")
        || input.contains("liqueur") {
            return Some("lick 'er? I 'ardly know 'er!".to_owned());
        }

        // TODO make this a lazy static.
        let er_regex = Regex::new(r"(\P{White_Space}+)er(s|ed|ing)?(\p{White_Space}|[\.!?,]|$)").unwrap();

        let mut best_word = "";
        for captures in er_regex.captures_iter(&input) {
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

        let mut best_word = best_word.to_owned();
        
        if // For example, "collid-er"
           best_word.ends_with("id")
        || (
            // pok-er => poke 'er
            best_word.ends_with("ok")
            // but, book-er => book 'er
            && !best_word.ends_with("ook")
        )
        {
            best_word.push('e');
        }

        best_word.push_str(" 'er? I 'ardly know 'er!");

        Some(best_word)
    }

    #[test]
    fn response_works_on_these_examples() {
        macro_rules! a {
            ($input: literal, $expected: expr) => {
                let expected: Option<&str> = $expected;
                let expected: Option<String> = expected.map(|s| s.to_owned());
                assert_eq!(response($input.to_owned()), expected);
            }
        }
        a!("", None);
        a!("booker", Some("book 'er? I 'ardly know 'er!"));
        a!("liquor", Some("lick 'er? I 'ardly know 'er!"));
        a!("Large Hadron Collider", Some("collide 'er? I 'ardly know 'er!"));
        a!("cupholder", Some("cuphold 'er? I 'ardly know 'er!"));
        a!("poker", Some("poke 'er? I 'ardly know 'er!"));
    }
}