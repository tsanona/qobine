use std::collections::HashSet;
use std::{cell::RefCell, rc::Rc, sync::Arc};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use qobuz_player_controls::ExitSender;
use qobuz_player_controls::VolumeReceiver;
use qobuz_player_controls::client::Client;
use qobuz_player_controls::controls::Controls;
use qobuz_player_controls::database::Database;
use qobuz_player_controls::tracklist::Tracklist;
use tokio::sync::mpsc;

use crate::UiEventSender;
use crate::ui::albums_page::{AlbumsPage, new_albums_page};
use crate::ui::artists_page::{ArtistsPage, new_artists_page};
use crate::ui::favorite_tracks_page::FavoriteTracksPage;
use crate::ui::playlists_page::{PlaylistsPage, new_playlists_page};
use crate::ui::preferences::build_preferences_menu;
use crate::ui::queue::QueuePage;
use crate::ui::search_page::SearchPage;
use crate::ui::{
    album_detail_page::AlbumHeaderInfo, artist_detail_page::ArtistHeaderInfo,
    playlist_detail_page::PlaylistHeaderInfo,
};

const SIDEBAR_QUEUE: u32 = 0;
const SIDEBAR_ALBUMS: u32 = 1;
const SIDEBAR_ARTISTS: u32 = 2;
const SIDEBAR_PLAYLISTS: u32 = 3;
const SIDEBAR_TRACKS: u32 = 4;

pub struct AppShell {
    root: adw::NavigationSplitView,
    client: Arc<Client>,
    spinner: gtk4::Spinner,
    waiting_label: gtk4::Label,
    albums_page: Rc<RefCell<AlbumsPage>>,
    artists_page: Rc<RefCell<ArtistsPage>>,
    playlists_page: Rc<RefCell<PlaylistsPage>>,
    favorite_tracks_page: FavoriteTracksPage,
    queue_page: QueuePage,
}

impl AppShell {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: &libadwaita::Application,
        client: Arc<Client>,
        controls: Controls,
        database: Arc<Database>,
        volume_receiver: VolumeReceiver,
        exit_sender: ExitSender,
        audio_cache_ttl_sender: mpsc::UnboundedSender<u32>,
        on_open_album: Rc<dyn Fn(AlbumHeaderInfo)>,
        on_open_artist: Rc<dyn Fn(ArtistHeaderInfo)>,
        on_open_playlist: Rc<dyn Fn(PlaylistHeaderInfo)>,
        ui_event_sender: UiEventSender,
    ) -> Self {
        let albums_page = Rc::new(RefCell::new(new_albums_page(on_open_album.clone())));
        let artists_page = Rc::new(RefCell::new(new_artists_page(on_open_artist.clone())));
        let playlists_page = Rc::new(RefCell::new(new_playlists_page(on_open_playlist.clone())));
        let favorite_tracks_page =
            FavoriteTracksPage::new(controls.clone(), client.clone(), ui_event_sender.clone());
        let queue_page = QueuePage::new(controls.clone(), client.clone(), ui_event_sender.clone());

        let search_page = Rc::new(RefCell::new(SearchPage::new(
            client.clone(),
            on_open_album,
            on_open_artist,
            on_open_playlist,
        )));

        let stack = adw::ViewStack::builder()
            .vexpand(true)
            .hexpand(true)
            .build();

        stack.add_named(queue_page.widget(), Some("queue"));
        stack.add_named(albums_page.borrow().widget(), Some("albums"));
        stack.add_named(artists_page.borrow().widget(), Some("artists"));
        stack.add_named(playlists_page.borrow().widget(), Some("playlists"));
        stack.add_named(favorite_tracks_page.widget(), Some("tracks"));
        stack.add_named(search_page.borrow().widget(), Some("search"));

        let spinner = gtk4::Spinner::new();
        spinner.start();
        spinner.set_visible(true);

        let waiting_label = gtk4::Label::builder()
            .label("Waiting for login...")
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .visible(true)
            .build();

        let spinner_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .halign(gtk4::Align::Center)
            .valign(gtk4::Align::Center)
            .build();

        spinner_box.append(&spinner);
        spinner_box.append(&waiting_label);

        let sidebar = adw::Sidebar::new();

        let queue_section = adw::SidebarSection::new();

        queue_section.append(
            adw::SidebarItem::builder()
                .title("Queue")
                .icon_name("open-menu-symbolic")
                .build(),
        );

        let library_section = adw::SidebarSection::new();
        library_section.set_title(Some("Library"));

        library_section.append(
            adw::SidebarItem::builder()
                .title("Albums")
                .icon_name("media-optical-symbolic")
                .build(),
        );

        library_section.append(
            adw::SidebarItem::builder()
                .title("Artists")
                .icon_name("system-users-symbolic")
                .build(),
        );

        library_section.append(
            adw::SidebarItem::builder()
                .title("Playlists")
                .icon_name("view-list-symbolic")
                .build(),
        );

        library_section.append(
            adw::SidebarItem::builder()
                .title("Tracks")
                .icon_name("folder-music-symbolic")
                .build(),
        );

        sidebar.append(queue_section);
        sidebar.append(library_section);

        let sidebar_header = adw::HeaderBar::new();
        sidebar_header.set_show_end_title_buttons(false);

        let sidebar_title = adw::WindowTitle::builder().build();
        sidebar_header.set_title_widget(Some(&sidebar_title));

        let content_header = adw::HeaderBar::new();

        let content_title = adw::WindowTitle::builder().title("Albums").build();
        content_header.set_title_widget(Some(&content_title));

        let filter_button: gtk4::ToggleButton = gtk4::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Filter")
            .css_classes(vec!["flat"])
            .visible(true)
            .build();

        content_header.pack_end(&filter_button);

        let filter_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Filter…")
            .hexpand(true)
            .build();

        filter_button.connect_toggled({
            let content_header = content_header.clone();
            let content_title = content_title.clone();
            let filter_entry = filter_entry.clone();

            let albums_page = albums_page.clone();
            let artists_page = artists_page.clone();
            let playlists_page = playlists_page.clone();

            move |button| {
                if button.is_active() {
                    content_header.set_title_widget(Some(&filter_entry));
                    filter_entry.grab_focus();
                } else {
                    content_header.set_title_widget(Some(&content_title));
                    filter_entry.set_text("");

                    albums_page.borrow().filter("");
                    artists_page.borrow().filter("");
                    playlists_page.borrow().filter("");
                }
            }
        });

        filter_entry.connect_changed({
            let albums_page = albums_page.clone();
            let artists_page = artists_page.clone();
            let playlists_page = playlists_page.clone();

            move |search_entry| {
                let query = search_entry.text();
                albums_page.borrow().filter(&query);
                artists_page.borrow().filter(&query);
                playlists_page.borrow().filter(&query);
            }
        });

        let search_entry = gtk4::SearchEntry::builder()
            .placeholder_text("Search…")
            .hexpand(true)
            .build();

        search_entry.connect_activate({
            let search_page = search_page.clone();
            move |e| {
                let q = e.text().to_string();
                search_page.borrow_mut().search(q);
            }
        });

        let search_button = gtk4::ToggleButton::builder()
            .icon_name("system-search-symbolic")
            .tooltip_text("Search")
            .build();

        let preferences_button = build_preferences_menu(
            app,
            controls,
            database,
            volume_receiver,
            exit_sender,
            audio_cache_ttl_sender,
        );

        sidebar_header.pack_start(&search_button);
        sidebar_header.pack_end(&preferences_button);

        let split_view = adw::NavigationSplitView::builder()
            .vexpand(true)
            .hexpand(true)
            .css_classes(vec!["view"])
            .valign(gtk4::Align::Fill)
            .halign(gtk4::Align::Fill)
            .build();

        let sidebar_toolbar = adw::ToolbarView::new();
        sidebar_toolbar.add_top_bar(&sidebar_header);
        sidebar_toolbar.set_content(Some(&sidebar));

        let content_toolbar = adw::ToolbarView::new();
        content_toolbar.add_top_bar(&content_header);

        let content_overlay = gtk4::Overlay::builder().vexpand(true).hexpand(true).build();

        content_overlay.set_child(Some(&stack));
        content_overlay.add_overlay(&spinner_box);

        content_toolbar.set_content(Some(&content_overlay));

        let sidebar_page = adw::NavigationPage::builder()
            .title("Library")
            .child(&sidebar_toolbar)
            .build();

        let content_page = adw::NavigationPage::builder()
            .title("Content")
            .child(&content_toolbar)
            .build();

        split_view.set_sidebar(Some(&sidebar_page));
        split_view.set_content(Some(&content_page));
        split_view.set_show_content(true);

        search_button.connect_clicked({
            let stack = stack.clone();

            move |button| {
                if button.is_active() {
                    stack.set_visible_child_name("search");
                } else {
                    stack.set_visible_child_name("albums");
                }
            }
        });

        sidebar.connect_selected_notify({
            let stack = stack.clone();
            let search_button = search_button.clone();

            move |sb| {
                let idx = sb.selected();

                if idx == gtk4::INVALID_LIST_POSITION {
                    return;
                }

                search_button.set_active(false);

                match idx {
                    SIDEBAR_QUEUE => stack.set_visible_child_name("queue"),
                    SIDEBAR_ALBUMS => stack.set_visible_child_name("albums"),
                    SIDEBAR_ARTISTS => stack.set_visible_child_name("artists"),
                    SIDEBAR_PLAYLISTS => stack.set_visible_child_name("playlists"),
                    SIDEBAR_TRACKS => stack.set_visible_child_name("tracks"),
                    _ => {}
                }
            }
        });

        stack.connect_visible_child_notify({
            let sidebar = sidebar.clone();
            let search_button = search_button.clone();
            let filter_button = filter_button.clone();
            let filter_entry = filter_entry.clone();
            let search_entry = search_entry.clone();
            let content_header = content_header.clone();
            let content_title = content_title.clone();

            move |stack| {
                let Some(visible_name) = stack.visible_child_name() else {
                    return;
                };

                match visible_name.as_str() {
                    "queue" => {
                        filter_button.set_active(false);
                        filter_button.set_visible(false);
                        search_button.set_active(false);

                        content_title.set_title("Queue");
                        content_header.set_title_widget(Some(&content_title));

                        if sidebar.selected() != SIDEBAR_QUEUE {
                            sidebar.set_selected(SIDEBAR_QUEUE);
                        }
                    }
                    "albums" => {
                        filter_button.set_visible(true);
                        search_button.set_active(false);

                        content_title.set_title("Albums");

                        if filter_button.is_active() {
                            content_header.set_title_widget(Some(&filter_entry));
                        } else {
                            content_header.set_title_widget(Some(&content_title));
                        }

                        if sidebar.selected() != SIDEBAR_ALBUMS {
                            sidebar.set_selected(SIDEBAR_ALBUMS);
                        }
                    }
                    "artists" => {
                        filter_button.set_visible(true);
                        search_button.set_active(false);

                        content_title.set_title("Artists");

                        if filter_button.is_active() {
                            content_header.set_title_widget(Some(&filter_entry));
                        } else {
                            content_header.set_title_widget(Some(&content_title));
                        }

                        if sidebar.selected() != SIDEBAR_ARTISTS {
                            sidebar.set_selected(SIDEBAR_ARTISTS);
                        }
                    }
                    "playlists" => {
                        filter_button.set_visible(true);
                        search_button.set_active(false);

                        content_title.set_title("Playlists");

                        if filter_button.is_active() {
                            content_header.set_title_widget(Some(&filter_entry));
                        } else {
                            content_header.set_title_widget(Some(&content_title));
                        }

                        if sidebar.selected() != SIDEBAR_PLAYLISTS {
                            sidebar.set_selected(SIDEBAR_PLAYLISTS);
                        }
                    }
                    "tracks" => {
                        filter_button.set_visible(true);
                        search_button.set_active(false);

                        content_title.set_title("Tracks");

                        if filter_button.is_active() {
                            content_header.set_title_widget(Some(&filter_entry));
                        } else {
                            content_header.set_title_widget(Some(&content_title));
                        }

                        if sidebar.selected() != SIDEBAR_TRACKS {
                            sidebar.set_selected(SIDEBAR_TRACKS);
                        }
                    }
                    "search" => {
                        filter_button.set_active(false);
                        filter_button.set_visible(false);

                        content_title.set_title("Search");
                        content_header.set_title_widget(Some(&search_entry));

                        search_entry.grab_focus();

                        if sidebar.selected() != gtk4::INVALID_LIST_POSITION {
                            sidebar.set_selected(gtk4::INVALID_LIST_POSITION);
                        }
                    }
                    _ => {}
                }
            }
        });

        sidebar.set_selected(SIDEBAR_ALBUMS);
        stack.set_visible_child_name("albums");

        Self {
            root: split_view,
            client,
            spinner,
            waiting_label,
            albums_page,
            artists_page,
            playlists_page,
            queue_page,
            favorite_tracks_page,
        }
    }

    pub fn widget(&self) -> &adw::NavigationSplitView {
        &self.root
    }

    pub fn reload(&self) {
        reload_favorites(
            self.client.clone(),
            &self.spinner,
            &self.waiting_label,
            &self.albums_page,
            &self.artists_page,
            &self.playlists_page,
            &self.favorite_tracks_page,
            &self.queue_page,
        );
    }

    pub fn tracklist_updated(&self, tracklist: &Tracklist) {
        self.queue_page.load(tracklist);
    }
}

#[allow(clippy::too_many_arguments)]
fn reload_favorites(
    client: Arc<Client>,
    spinner: &gtk4::Spinner,
    waiting_label: &gtk4::Label,
    albums_page: &Rc<RefCell<AlbumsPage>>,
    artists_page: &Rc<RefCell<ArtistsPage>>,
    playlists_page: &Rc<RefCell<PlaylistsPage>>,
    favorite_tracks_page: &FavoriteTracksPage,
    queue_page: &QueuePage,
) {
    let albums_page = albums_page.clone();
    let artists_page = artists_page.clone();
    let playlists_page = playlists_page.clone();
    let favorite_tracks_page = favorite_tracks_page.clone();
    let queue_page = queue_page.clone();

    let spinner = spinner.clone();
    let waiting_label = waiting_label.clone();

    waiting_label.set_visible(false);
    spinner.set_visible(true);
    spinner.start();

    glib::MainContext::default().spawn_local(async move {
        match client.favorites().await {
            Ok(favorites) => {
                spinner.set_visible(false);
                spinner.stop();

                albums_page.borrow_mut().load(favorites.albums);
                artists_page.borrow_mut().load(favorites.artists);

                playlists_page
                    .borrow_mut()
                    .load(favorites.playlists.into_iter().map(|x| x.into()).collect());

                let favorite_tracks: HashSet<_> = favorites.tracks.iter().map(|x| x.id).collect();

                favorite_tracks_page.load(favorites.tracks);

                queue_page.favorite_tracks_changed(favorite_tracks);
            }
            Err(err) => {
                spinner.set_visible(false);
                spinner.stop();
                tracing::error!("{err}");
            }
        }
    });
}
