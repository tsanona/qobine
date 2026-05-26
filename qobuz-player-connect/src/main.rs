use qobuz_player_cli::{
    DelayArgs, SharedArgs, SharedCommands, create_player, default_audio_cache,
    default_audio_quality, get_client, handle_shared_commands, spawn_clean_up,
};
use std::sync::Arc;
use tokio::sync::broadcast;

use clap::Parser;
use qobuz_player_controls::{AppResult, database::Database, notification::NotificationBroadcast};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Arguments {
    #[clap(flatten)]
    shared: SharedArgs,

    #[clap(flatten)]
    delay: DelayArgs,

    #[cfg(feature = "gpio")]
    #[clap(flatten)]
    gpio: qobuz_player_cli::GpioArgs,

    #[cfg(feature = "mqtt")]
    #[clap(flatten)]
    mqtt_config: qobuz_player_mqtt::MqttArgs,

    #[clap(flatten)]
    connect_config: qobuz_player_cli::ConnectArgs,

    #[clap(subcommand)]
    command: Option<SharedCommands>,
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(()) => {}
        Err(err) => {
            error_exit(err);
        }
    }
}

pub async fn run() -> AppResult<()> {
    let args = Arguments::parse();
    let database = Arc::new(Database::new().await?);
    let headless = true;

    if let Some(command) = args.command {
        handle_shared_commands(command, &database, headless).await?;
        return Ok(());
    }

    let (_, exit_receiver) = broadcast::channel(5);

    let max_audio_quality = default_audio_quality(&database, args.shared.max_audio_quality).await?;
    let client = get_client(
        &database,
        max_audio_quality,
        args.shared.file_based_streaming,
        headless,
    )
    .await?;
    let client = Arc::new(client);

    let broadcast = Arc::new(NotificationBroadcast::new());
    let audio_cache = default_audio_cache(args.shared.audio_cache);

    let mut player = create_player(
        audio_cache,
        database.clone(),
        client.clone(),
        broadcast.clone(),
        args.delay.state_change_delay_ms,
        args.delay.sample_rate_change_delay_ms,
        args.shared.output_device_id,
    )
    .await?;

    #[cfg(feature = "gpio")]
    if args.gpio.gpio {
        let status_receiver = player.status();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_gpio::init(status_receiver).await {
                error_exit(e);
            }
        });
    }

    #[cfg(feature = "mqtt")]
    {
        let controls = player.controls();
        let status_receiver = player.status();
        let volume_reciever = player.volume();
        let track_list_reciever = player.tracklist();
        tokio::spawn(async move {
            if let Err(e) = qobuz_player_mqtt::init(
                controls,
                status_receiver,
                volume_reciever,
                track_list_reciever,
                args.mqtt_config,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    {
       let app_id = client.app_id().await?;
        let controls = player.controls();
        let position_receiver = player.position();
        let tracklist_receiver = player.tracklist();
        let status_receiver = player.status();
        let volume_receiver = player.volume();

        tokio::spawn(async move {
            if let Err(e) = qobuz_player_connect::init(
                &app_id,
                args.connect_config.connect_name,
                args.connect_config.connect_port,
                controls,
                position_receiver,
                tracklist_receiver,
                status_receiver,
                volume_receiver,
                max_audio_quality,
            )
            .await
            {
                error_exit(e);
            }
        });
    }

    spawn_clean_up(database, args.shared.audio_cache_time_to_live);
    player.player_loop(exit_receiver).await?;

    Ok(())
}

fn error_exit(error: impl std::error::Error) {
    eprintln!("{error}");
    std::process::exit(1);
}