use std::time::Duration;

use paho_mqtt as mqtt;

use qobuz_player_controls::{
    AppResult, StatusReceiver, TracklistReceiver, VolumeReceiver, controls::Controls,
};
use tracing::{error, info, warn};

const QOS: i32 = 0;

#[cfg(feature = "cli")]
fn check_server_uri(uri: &str) -> Result<String, String> {
    if let Some((protocol, host_port)) = uri.trim().split_once("://")
        && ["ssl", "tcp"].contains(&protocol)
        && let Some((host, port)) = host_port.split_once(":")
        && port.chars().all(char::is_numeric)
    {
        return Ok(format!("{protocol}://{host}:{port}"));
    }
    Err("Not a valid uri".into())
}

#[cfg_attr(feature = "cli", derive(clap::Args))]
pub struct MqttConfig {
    /// Sets the the URI to the MQTT broker.
    ///
    /// Expects a string in the form {tcp|ssl}://{host}:{port},
    /// where host can be an IP address or domain name.
    #[cfg_attr(feature = "cli", clap(long, value_parser = check_server_uri, env))]
    mqtt_server_uri: String,
    /// Sets the user name for authentication with the broker.
    #[cfg_attr(feature = "cli", clap(long, requires("mqtt_password")))]
    mqtt_user_name: Option<String>,
    /// Sets the password for authentication with the broker. This works with the user name.
    #[cfg_attr(feature = "cli", clap(long, requires("mqtt_user_name")))]
    mqtt_password: Option<String>,
    /// Sets the client identifier string
    /// that is sent to the server.
    ///
    /// The client ID is a unique name to
    /// identify the client to the server,
    /// which can be used if the client
    /// desires the server to hold state
    /// about the session.
    ///
    /// If the client
    /// requests a clean session, this
    /// can be an empty string, in which
    /// case the server will assign a
    /// random name for the client.
    #[cfg_attr(feature = "cli", clap(long, default_value = ""))]
    mqtt_client_id: String,
    /// Sets the mqtt topic.
    ///
    /// Will publish in {topic}/pub and recieve messages from {topic}/sub.
    #[cfg_attr(feature = "cli", clap(long, default_value = "qobine"))]
    mqtt_topic: String,
}

fn prepared_message<V: Into<Vec<u8>>>(topic: &str, payload: V) -> mqtt::Message {
    mqtt::Message::new(format!("{}/pub", topic), payload, QOS)
}

fn publish_serializible<T: serde::Serialize>(
    mqtt_client: &mqtt::AsyncClient,
    topic: &str,
    serializible: &T,
) {
    match serde_json::to_string(serializible) {
        Ok(json) => mqtt_client
            .publish(prepared_message(topic, json))
            .wait()
            .unwrap_or_else(|e| error!("Could not publish message: {e}")),
        Err(e) => error!("Tracklist could not be serialized to json: {e}"),
    }
}

pub async fn init(
    controls: Controls,
    mut status_receiver: StatusReceiver,
    mut volume_receiver: VolumeReceiver,
    mut track_list_receiver: TracklistReceiver,
    args: MqttConfig,
) -> AppResult<(), mqtt::Error> {
    let mqtt_create_opts = mqtt::CreateOptionsBuilder::new()
        .server_uri(args.mqtt_server_uri)
        .allow_disconnected_send_at_anytime(true)
        .send_while_disconnected(true)
        .max_buffered_messages(10)
        .delete_oldest_messages(true)
        .client_id(args.mqtt_client_id)
        .finalize();

    let topic = args.mqtt_topic.clone();
    let mut mqtt_client = mqtt::AsyncClient::new(mqtt_create_opts)?;
    mqtt_client.set_connected_callback(move |mqtt_client| {
        _ = mqtt_client.subscribe(format!("{}/sub", topic), QOS).wait();
    });

    let subscribe_stream = mqtt_client.get_stream(1);

    let mqtt_connect_opts = {
        let mut connect_options_builder = mqtt::ConnectOptionsBuilder::new();
        connect_options_builder
            .keep_alive_interval(Duration::from_secs(20))
            .automatic_reconnect(Duration::from_millis(1), Duration::from_secs(24 * 60 * 60))
            .clean_session(true);
        if let Some(user_name) = args.mqtt_user_name
            && let Some(password) = args.mqtt_password
        {
            connect_options_builder
                .user_name(user_name)
                .password(password);
        }
        connect_options_builder.finalize()
    };

    mqtt_client.connect(mqtt_connect_opts).wait()?;

    loop {
        tokio::select! {
            Ok(()) = status_receiver.changed(), if mqtt_client.is_connected() => {
                publish_serializible(&mqtt_client, &args.mqtt_topic, &*status_receiver.borrow_and_update());
            },
            Ok(()) = volume_receiver.changed(), if mqtt_client.is_connected() => {
                let volume = *volume_receiver.borrow_and_update();
                let volume = serde_json::json!({"volume": volume}).to_string();
                mqtt_client.publish(prepared_message(&args.mqtt_topic, volume));
            }
            Ok(()) = track_list_receiver.changed(), if mqtt_client.is_connected() => {
                publish_serializible(&mqtt_client, &args.mqtt_topic, &*track_list_receiver.borrow_and_update());
            }
            Ok(Some(message)) = subscribe_stream.recv() => {
                match message.payload_str().as_ref() {
                    "play" => controls.play(),
                    "pause" => controls.pause(),
                    "playpause" => controls.play_pause(),
                    "jumpbackward" => controls.jump_forward(),
                    "jumpforward" => controls.jump_forward(),
                    "next" => controls.next(),
                    "previous" => controls.previous(),
                    // TODO: volumeup, volumedown
                    unrecognized_command => warn!("Unrecognized mqtt command: {unrecognized_command}")
                }
            }
            else => {
                if !mqtt_client.is_connected() {
                    info!("Lost connection to mqtt.\n Reconnecting...");
                    mqtt_client.reconnect().wait()?;
                    info!("Reconnection Successfull.");

                } else {
                    info!("Player has stoped.\n Gracefully shutting down mqtt.");
                    mqtt_client.disconnect(None).wait()?;
                    return Ok(())
                }
            }
        }
    }
}
