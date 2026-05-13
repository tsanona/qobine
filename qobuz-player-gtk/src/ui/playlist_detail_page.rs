use std::{cell::RefCell, collections::HashSet, rc::Rc, sync::Arc};

use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::{
    TracklistReceiver, client::Client, controls::Controls, tracklist::PlayingEntity,
};

use crate::{
    UiEventSender,
    ui::{
        DetailPage, DetailPageType, build_track_row,
        detail_page::{DetailType, build_detail_header, build_detail_scaffold},
        format_time, set_image_from_url,
    },
};

#[derive(Clone, Debug)]
pub struct PlaylistHeaderInfo {
    pub id: u32,
}

pub struct PlaylistDetailPage {
    page: adw::NavigationPage,

    client: Arc<Client>,
    controls: Controls,
    tracklist_receiver: TracklistReceiver,
    playlist_id: u32,

    stack: gtk4::Stack,

    cover: gtk4::Image,
    title: gtk4::Label,
    meta: gtk4::Label,

    tracks_list: gtk4::ListBox,

    current_selected_index: Rc<RefCell<Option<usize>>>,

    loaded: RefCell<bool>,
    ui_event_sender: UiEventSender,
}

impl PlaylistDetailPage {
    pub fn new(
        playlist_id: u32,
        controls: Controls,
        client: Arc<Client>,
        tracklist_receiver: TracklistReceiver,
        ui_event_sender: UiEventSender,
    ) -> Self {
        let empty_title = gtk4::Box::builder().hexpand(true).build();
        let nav_bar = adw::HeaderBar::builder().title_widget(&empty_title).build();

        let title = gtk4::Label::builder()
            .wrap(true)
            .css_classes(vec!["title-1"])
            .build();

        let meta = gtk4::Label::builder()
            .wrap(true)
            .css_classes(vec!["dim-label"])
            .build();

        let play_button = gtk4::Button::builder()
            .label("Play")
            .icon_name("media-playback-start-symbolic")
            .css_classes(vec!["suggested-action", "pill"])
            .build();

        play_button.connect_clicked({
            let controls = controls.clone();
            move |_| {
                controls.play_playlist(playlist_id, 0, false);
            }
        });

        let shuffle_button = gtk4::Button::builder()
            .label("Shuffle")
            .icon_name("media-playlist-shuffle-symbolic")
            .css_classes(vec!["pill"])
            .build();

        shuffle_button.connect_clicked({
            let controls = controls.clone();
            move |_| {
                controls.play_playlist(playlist_id, 0, true);
            }
        });

        let header = build_detail_header(
            client.clone(),
            controls.clone(),
            ui_event_sender.clone(),
            300,
            vec![title.clone().upcast(), meta.clone().upcast()],
            vec![
                play_button.clone().upcast(),
                shuffle_button.clone().upcast(),
            ],
            DetailType::Playlist(playlist_id),
        );

        let scaffold = build_detail_scaffold(&header.header_section, {
            let controls = controls.clone();
            move |index| {
                controls.play_playlist(playlist_id, index, false);
            }
        });

        let cover = header.cover.clone();
        let stack = scaffold.stack.clone();
        let tracks_list = scaffold.tracks_list.clone();

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&nav_bar);
        toolbar.set_content(Some(&stack));

        let page = adw::NavigationPage::builder()
            .title("Playlist")
            .child(&toolbar)
            .build();

        let s = Self {
            page,
            client,
            controls,
            tracklist_receiver,
            playlist_id,
            stack,
            cover,
            title,
            meta,
            tracks_list,
            loaded: RefCell::new(false),
            current_selected_index: Rc::new(RefCell::new(None)),
            ui_event_sender,
        };

        s.load_playlist();

        s
    }

    fn load_playlist(&self) {
        if *self.loaded.borrow() {
            return;
        }
        *self.loaded.borrow_mut() = true;

        let client = self.client.clone();
        let ui_event_sender = self.ui_event_sender.clone();
        let controls = self.controls.clone();
        let playlist_id = self.playlist_id;

        let stack = self.stack.clone();
        let cover = self.cover.clone();
        let title = self.title.clone();
        let meta = self.meta.clone();
        let tracks_list = self.tracks_list.clone();
        let tracklist_receiver = self.tracklist_receiver.clone();
        let current_playing_index = self.current_selected_index.clone();

        stack.set_visible_child_name("loading");

        glib::MainContext::default().spawn_local(async move {
            match client.playlist(playlist_id).await {
                Ok(playlist) => {
                    title.set_label(&playlist.title);

                    let dur_str = format_time(playlist.duration_seconds);
                    meta.set_label(&dur_str.to_string());

                    set_image_from_url(playlist.image.as_deref(), &cover);

                    clear_listbox(&tracks_list);

                    let favorite_tracks = client
                        .favorites()
                        .await
                        .map(|x| x.tracks.into_iter().map(|x| x.id).collect())
                        .unwrap_or(HashSet::new());

                    for track in playlist.tracks {
                        let row = build_track_row(
                            &track,
                            true,
                            true,
                            false,
                            controls.clone(),
                            client.clone(),
                            ui_event_sender.clone(),
                            &favorite_tracks,
                        );
                        tracks_list.append(&row);
                    }

                    let playing_entity = tracklist_receiver.borrow().current_playing_entity();
                    if let Some(playing_entity) = playing_entity {
                        update_current_playing(
                            &playing_entity,
                            playlist_id,
                            &current_playing_index,
                            &tracks_list,
                        );
                    }
                    stack.set_visible_child_name("content");
                }
                Err(err) => {
                    tracing::error!("Failed to load playlist {playlist_id}: {err}");

                    clear_listbox(&tracks_list);

                    let label = gtk4::Label::builder()
                        .label("Failed to load playlist.")
                        .xalign(0.0)
                        .margin_top(12)
                        .margin_bottom(12)
                        .margin_start(12)
                        .margin_end(12)
                        .css_classes(vec!["dim-label"])
                        .build();

                    let row = adw::ActionRow::builder().child(&label).build();
                    tracks_list.append(&row);

                    stack.set_visible_child_name("content");
                }
            }
        });
    }
}

impl DetailPage for PlaylistDetailPage {
    fn page(&self) -> &adw::NavigationPage {
        &self.page
    }

    fn update_current_playing(&self, playing_entity: PlayingEntity) {
        update_current_playing(
            &playing_entity,
            self.playlist_id,
            &self.current_selected_index,
            &self.tracks_list,
        );
    }

    fn detail_type(&self) -> DetailPageType {
        DetailPageType::Playlist(self.playlist_id)
    }
}

fn update_current_playing(
    playing_entity: &PlayingEntity,
    playlist_id: u32,
    current_selected_index: &Rc<RefCell<Option<usize>>>,
    tracks_list: &gtk4::ListBox,
) {
    let playing = match playing_entity {
        PlayingEntity::Playlist(p) => p,
        _ => return,
    };

    if playing.playlist_id != playlist_id {
        tracks_list.unselect_all();
        *current_selected_index.borrow_mut() = None;
        return;
    }

    let idx = playing.index;
    *current_selected_index.borrow_mut() = Some(idx);

    if let Some(row) = tracks_list.row_at_index(idx as i32) {
        tracks_list.select_row(Some(&row));
    } else {
        tracks_list.unselect_all();
    }
}

fn clear_listbox(list: &gtk4::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
