use std::sync::Arc;

use futures::executor::block_on;
use glib::clone;
use gtk::gio;
use gtk::prelude::*;
use gtk4 as gtk;

use adw::prelude::*;
use libadwaita as adw;
use qobuz_player_controls::AudioQuality;
use qobuz_player_controls::ExitSender;
use qobuz_player_controls::VolumeReceiver;
use qobuz_player_controls::controls::Controls;
use qobuz_player_controls::database::Configuration;
use qobuz_player_controls::database::Database;
use tokio::sync::mpsc;

pub fn build_preferences_menu(
    app: &adw::Application,
    controls: Controls,
    database: Arc<Database>,
    volume_receiver: VolumeReceiver,
    exit_sender: ExitSender,
    audio_cache_ttl_sender: mpsc::Sender<u32>,
) -> gtk::MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("Preferences"), Some("app.preferences"));

    let menu_model = gtk::PopoverMenu::from_model(Some(&menu));

    let button = gtk::MenuButton::new();
    button.set_icon_name("open-menu-symbolic");
    button.set_popover(Some(&menu_model));

    let action = gio::SimpleAction::new("preferences", None);

    action.connect_activate({
        clone!(
            #[weak]
            app,
            #[strong]
            controls,
            #[weak]
            database,
            #[strong]
            volume_receiver,
            #[strong]
            exit_sender,
            #[strong]
            audio_cache_ttl_sender,
            move |_, _| {
                show_preferences_dialog(
                    &app,
                    controls.clone(),
                    database,
                    volume_receiver.clone(),
                    exit_sender.clone(),
                    audio_cache_ttl_sender.clone(),
                );
            }
        )
    });
    app.add_action(&action);

    button
}

fn show_preferences_dialog(
    app: &adw::Application,
    controls: Controls,
    database: Arc<Database>,
    volume_receiver: VolumeReceiver,
    exit_sender: ExitSender,
    audio_cache_ttl_sender: mpsc::Sender<u32>,
) {
    let dialog = adw::PreferencesDialog::new();

    dialog.add(&preferences_page(
        app,
        controls,
        database,
        volume_receiver,
        exit_sender,
        audio_cache_ttl_sender,
    ));
    dialog.present(app.active_window().as_ref());
}

fn preferences_page(
    app: &adw::Application,
    controls: Controls,
    database: Arc<Database>,
    volume_receiver: VolumeReceiver,
    exit_sender: ExitSender,
    audio_cache_ttl_sender: mpsc::Sender<u32>,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Preferences");

    let configuration = block_on(database.get_configuration()).unwrap();

    page.add(&cache_group(
        app,
        database.clone(),
        exit_sender.clone(),
        audio_cache_ttl_sender,
        &configuration,
    ));
    page.add(&audio_group(
        controls,
        database,
        volume_receiver,
        exit_sender.clone(),
        &configuration,
    ));
    page.add(&logout_group(exit_sender));

    page
}

fn cache_group(
    app: &adw::Application,
    database: Arc<Database>,
    exit_sender: ExitSender,
    audio_cache_ttl_sender: mpsc::Sender<u32>,
    configuration: &Configuration,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Cache");

    let initial_cache_dir = configuration.cache_directory.clone();

    let row = adw::ActionRow::new();
    row.set_title("Cache directory");
    row.set_subtitle_lines(1);

    let initial_subtitle = initial_cache_dir.display().to_string();

    row.set_subtitle(&initial_subtitle);
    row.set_activatable(true);

    row.connect_activated({
        let exit_sender = exit_sender.clone();
        glib::clone!(
            #[weak]
            app,
            #[weak]
            row,
            #[strong]
            initial_cache_dir,
            #[weak]
            database,
            #[strong]
            exit_sender,
            move |_| {
                let file_dialog = gtk::FileDialog::builder()
                    .title("Select cache directory")
                    .modal(true)
                    .build();

                let gfile = gio::File::for_path(initial_cache_dir.clone());
                file_dialog.set_initial_folder(Some(&gfile));

                file_dialog.select_folder(
                    app.active_window().as_ref(),
                    gio::Cancellable::NONE,
                    glib::clone!(
                        #[weak]
                        row,
                        #[weak]
                        database,
                        #[strong]
                        exit_sender,
                        move |result| {
                            if let Ok(folder) = result
                                && let Some(path) = folder.path()
                            {
                                row.set_subtitle(&path.display().to_string());
                                // TODO: Confirm box
                                // TODO: Instead of exit, respawn clean up
                                block_on(database.set_cache_directory(path)).unwrap();
                                exit_sender.send(true).unwrap();
                            }
                        }
                    ),
                );
            }
        )
    });

    group.add(&row);
    group.add(&cache_ttl_row(database, audio_cache_ttl_sender));

    group
}

fn cache_ttl_row(
    database: Arc<Database>,
    audio_cache_ttl_sender: mpsc::Sender<u32>,
) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title("Cache time to live");

    let model = gtk::StringList::new(&["Disabled", "1 hour", "1 month", "3 months"]);

    row.set_model(Some(&model));
    row.set_selected(0);

    row.connect_selected_notify(move |r| {
        let hours = match r.selected() {
            0 => 0,
            1 => 1,
            2 => 24 * 30,
            3 => 24 * 90,
            _ => 0,
        };
        block_on(database.set_cache_ttl_hours(hours)).unwrap();
        block_on(audio_cache_ttl_sender.send(hours)).unwrap();
    });

    row
}

fn audio_group(
    controls: Controls,
    database: Arc<Database>,
    volume_receiver: VolumeReceiver,
    exit_sender: ExitSender,
    configuration: &Configuration,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Audio");

    let initial_selected = match configuration.max_audio_quality {
        AudioQuality::Mp3 => 0,
        AudioQuality::CD => 1,
        AudioQuality::HIFI96 => 2,
        AudioQuality::HIFI192 => 3,
    };

    let quality = adw::ComboRow::builder().selected(initial_selected).build();
    quality.set_title("Audio quality");

    let model = gtk::StringList::new(&[
        AudioQuality::Mp3.to_label_str(),
        AudioQuality::CD.to_label_str(),
        AudioQuality::HIFI96.to_label_str(),
        AudioQuality::HIFI192.to_label_str(),
    ]);

    quality.set_model(Some(&model));

    quality.connect_selected_notify(move |r| {
        let value = match r.selected() {
            0 => AudioQuality::Mp3,
            1 => AudioQuality::CD,
            2 => AudioQuality::HIFI96,
            3 => AudioQuality::HIFI192,
            _ => AudioQuality::HIFI192,
        };
        // TODO: Confirm box
        block_on(database.set_max_audio_quality(value)).unwrap();
        // TODO: Instead of exit, update the player. Easy
        exit_sender.send(true).unwrap();
    });

    group.add(&quality);

    let volume = adw::ActionRow::new();
    volume.set_title("Volume");

    let initial_value = volume_receiver.borrow();
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.1);
    scale.set_value(*initial_value as f64);
    scale.set_hexpand(true);

    scale.connect_value_changed(clone!(move |s| {
        controls.set_volume(s.value() as f32);
    }));

    volume.add_suffix(&scale);
    volume.set_activatable_widget(Some(&scale));

    group.add(&volume);
    group
}

fn logout_group(exit_sender: ExitSender) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();

    let logout = adw::ButtonRow::new();
    logout.set_title("Log out");
    logout.add_css_class("destructive-action");

    logout.connect_activated(move |_| {
        exit_sender.send(true).unwrap();
    });

    group.add(&logout);
    group
}
