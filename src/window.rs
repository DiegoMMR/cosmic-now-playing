// Mandatory COSMIC imports
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use cosmic::app::Core;
use cosmic::iced::{
    Background,
    Color,
    ContentFit,
    platform_specific::shell::commands::popup::{destroy_popup, get_popup},
    stream::channel,
    window::Id,
    Length,
    Limits,
    Subscription,
};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use mpris::{Event as MprisEvent, PlayerFinder};
use cosmic::iced_runtime::core::window;
use cosmic::{Action, Element, Task};

// Widgets we're going to use
use cosmic::widget::{button, button::Catalog, column, icon, text, Row};

use crate::metadata::{now_playing_from_player, now_playing_snapshot, NowPlayingData};
use crate::player::{album_art_path_from_metadata, playback_state_from_player, with_active_player};

// Every COSMIC Application and Applet MUST have an ID
const ID: &str = "com.github.DiegoMMR.CosmicExtAppletNowPlaying";

/*
*  Every COSMIC model must be a struct data type.
*  Mandatory fields for a COSMIC Applet are core and popup.
*  Core is the core settings that allow it to interact with COSMIC
*  and popup, as you'll see later, is the field that allows us to open
*  and close the applet.
*
*  Next we have our custom field that we will manipulate the value of based
*  on the message we send.
*/
#[derive(Default)]
pub struct Window {
    core: Core,
    popup: Option<Id>,
    now_playing_text: String,
    now_playing_title: String,
    now_playing_artist: String,
    playback_state: PlaybackState,
    album_art_path: Option<PathBuf>,
    album_color: Option<Color>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
    #[default]
    Unknown,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,         // Mandatory for open and close the applet
    PopupClosed(Id),     // Mandatory for the applet to know if it's been closed
    NowPlayingChanged(NowPlayingData),
    PreviousTrack,
    TogglePlayPause,
    NextTrack,
}

impl cosmic::Application for Window {
    /*
     *  Executors are a mandatory thing for both COSMIC Applications and Applets.
     *  They're basically what allows for multi-threaded async operations for things that
     *  may take too long and block the thread the GUI is running on. This is also where
     *  Tasks take place.
     */
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = (); // Honestly not sure what these are for.
    type Message = Message; // These are setting the application messages to our Message enum
    const APP_ID: &'static str = ID; // This is where we set our const above to the actual ID

    // Setup the immutable core functionality.
    fn core(&self) -> &Core {
        &self.core
    }

    // Set up the mutable core functionality.
    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    // Initialize the applet
    /*
     *  The parameters are the Core and flags (again not sure what to do with these).
     *  The function returns our model struct initialized and an Option<Task<Action<Self::Message>>>,
     *  in this case there is no command so it returns a None value with the type of Task in its place.
     */
    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let initial = now_playing_snapshot();

        let window = Window {
            core,                 // Set the incoming core
            now_playing_text: initial.text,
            now_playing_title: initial.title,
            now_playing_artist: initial.artist,
            playback_state: initial.state,
            album_color: dominant_album_color(initial.album_art_path.as_ref()),
            album_art_path: initial.album_art_path,
            ..Default::default()  // Set everything else to the default values
        };

        (window, Task::none())
    }

    // Create what happens when the applet is closed
    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        // Pass the PopupClosed message to the update function
        Some(Message::PopupClosed(id))
    }

    // Here is the update function, it's the one that handles all of the messages that
    // are passed within the applet.
    fn update(&mut self, message: Message) -> Task<Action<Self::Message>> {
        // match on what message was sent
        match message {
            // Handle the TogglePopup message
            Message::TogglePopup => {
                // Close the popup
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    // Create and "open" the popup
                    let new_id = Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(370.0)
                        .min_width(200.0)
                        .min_height(200.0)
                        .max_height(1080.0);

                    get_popup(popup_settings)
                };
            }
            // Unset the popup field after it's been closed
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::NowPlayingChanged(data) => {
                self.now_playing_text = data.text;
                self.now_playing_title = data.title;
                self.now_playing_artist = data.artist;
                self.playback_state = data.state;
                self.album_color = dominant_album_color(data.album_art_path.as_ref());
                self.album_art_path = data.album_art_path;
            }
            Message::PreviousTrack => {
                with_active_player(|player| {
                    let _ = player.previous();
                });
            }
            Message::TogglePlayPause => {
                with_active_player(|player| {
                    let _ = player.play_pause();
                });
            }
            Message::NextTrack => {
                with_active_player(|player| {
                    let _ = player.next();
                });
            }
        }
        Task::none() // Again not doing anything that requires multi-threading here.
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::run(|| {
            channel(
                64,
                |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                std::thread::spawn(move || {
                    let mut last_sent = String::new();
                    let mut last_state = PlaybackState::Unknown;
                    let mut last_art: Option<PathBuf> = None;

                    loop {
                        let finder = match PlayerFinder::new() {
                            Ok(finder) => finder,
                            Err(_) => {
                                std::thread::sleep(Duration::from_millis(1000));
                                continue;
                            }
                        };

                        let player = match finder.find_active() {
                            Ok(player) => player,
                            Err(_) => {
                                if last_sent != "Nothing playing" || last_state != PlaybackState::Stopped {
                                    last_sent = "Nothing playing".to_string();
                                    last_state = PlaybackState::Stopped;
                                    last_art = None;
                                    while output
                                        .try_send(Message::NowPlayingChanged(NowPlayingData {
                                            text: last_sent.clone(),
                                            title: "Nothing playing".to_string(),
                                            artist: String::new(),
                                            state: last_state,
                                            album_art_path: None,
                                        }))
                                        .is_err()
                                    {
                                        std::thread::sleep(Duration::from_millis(10));
                                    }
                                }

                                std::thread::sleep(Duration::from_millis(1000));
                                continue;
                            }
                        };

                        let current = now_playing_from_player(&player);
                        let current_state = current.state;
                        let current_art = current.album_art_path.clone();
                        if current.text != last_sent || current_state != last_state || current_art != last_art {
                            last_sent = current.text.clone();
                            last_state = current_state;
                            last_art = current_art.clone();
                            while output
                                .try_send(Message::NowPlayingChanged(current.clone()))
                                .is_err()
                            {
                                std::thread::sleep(Duration::from_millis(10));
                            }
                        }

                        let mut events = match player.events() {
                            Ok(events) => events,
                            Err(_) => {
                                std::thread::sleep(Duration::from_millis(300));
                                continue;
                            }
                        };

                        for event in &mut events {
                            match event {
                                Ok(MprisEvent::TrackChanged(metadata)) => {
                                    let title = metadata.title().unwrap_or("Unknown");
                                    let artist = metadata
                                        .artists()
                                        .and_then(|a| a.first().copied())
                                        .unwrap_or("Unknown");
                                    let text = format!("{} - {}", title, artist);
                                    let state = playback_state_from_player(&player);
                                    let art = album_art_path_from_metadata(&metadata);

                                    if text != last_sent || state != last_state || art != last_art {
                                        last_sent = text.clone();
                                        last_state = state;
                                        last_art = art.clone();
                                        while output
                                            .try_send(Message::NowPlayingChanged(NowPlayingData {
                                                text: text.clone(),
                                                title: title.to_string(),
                                                artist: artist.to_string(),
                                                state,
                                                album_art_path: art.clone(),
                                            }))
                                            .is_err()
                                        {
                                            std::thread::sleep(Duration::from_millis(10));
                                        }
                                    }
                                }
                                Ok(MprisEvent::Playing)
                                | Ok(MprisEvent::Paused)
                                | Ok(MprisEvent::Stopped) => {
                                    let data = now_playing_from_player(&player);
                                    let text = data.text.clone();
                                    let state = data.state;
                                    let art = data.album_art_path.clone();

                                    if text != last_sent || state != last_state || art != last_art {
                                        last_sent = text;
                                        last_state = state;
                                        last_art = art.clone();
                                        while output
                                            .try_send(Message::NowPlayingChanged(data.clone()))
                                            .is_err()
                                        {
                                            std::thread::sleep(Duration::from_millis(10));
                                        }
                                    }
                                }
                                Ok(MprisEvent::PlayerShutDown) | Err(_) => break,
                                _ => {}
                            }
                        }

                        std::thread::sleep(Duration::from_millis(200));
                    }
                });
                },
            )
        })
    }

    /*
     *  For an applet, the view function describes what an applet looks like. There's a
     *  secondary view function (view_window) that shows the widgets in the popup when it's
     *  opened.
     */
    fn view(&self) -> Element<'_, Message> {
        if !self.has_active_media() {
            return self
                .core
                .applet
                .autosize_window(text(""))
                .into();
        }

        let size = self.core.applet.suggested_size(true);
        let pad = self.core.applet.suggested_padding(true);
        let transport_icon = match self.playback_state {
            PlaybackState::Playing => "media-playback-pause-symbolic",
            PlaybackState::Paused | PlaybackState::Stopped | PlaybackState::Unknown => {
                "media-playback-start-symbolic"
            }
        };

        let row_content = Row::new()
            .spacing(pad.0)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .push(icon::from_name(transport_icon).size(size.0))
            .push(
                text(self.now_playing_text.as_str())
                    .size(size.0.saturating_sub(1))
                    .width(Length::Fixed(260.0))
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
            );

        let album_color = self.album_color;
        let content = button::custom(row_content)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .class(cosmic::theme::Button::Custom {
                active: Box::new(move |focused, theme| {
                    let base = theme.active(focused, false, &cosmic::theme::Button::AppletIcon);
                    style_with_optional_album_color(base, album_color)
                }),
                disabled: Box::new(move |theme| {
                    let base = theme.disabled(&cosmic::theme::Button::AppletIcon);
                    style_with_optional_album_color(base, album_color)
                }),
                hovered: Box::new(move |focused, theme| {
                    let base = theme.hovered(focused, false, &cosmic::theme::Button::AppletIcon);
                    style_with_optional_album_color(base, album_color.map(|c| shift_color(c, 0.07)))
                }),
                pressed: Box::new(move |focused, theme| {
                    let base = theme.pressed(focused, false, &cosmic::theme::Button::AppletIcon);
                    style_with_optional_album_color(base, album_color.map(|c| shift_color(c, -0.08)))
                }),
            })
            .on_press(Message::TogglePopup);

        self.core
            .applet
            .autosize_window(content)
            .into()
    }

    // The actual GUI window for the applet. It's a popup.
    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        if !self.has_active_media() {
            return self.core.applet.popup_container(text("")).into();
        }

        let size = self.core.applet.suggested_size(true);
        let pad = self.core.applet.suggested_padding(true);
        let transport_icon = match self.playback_state {
            PlaybackState::Playing => "media-playback-pause-symbolic",
            PlaybackState::Paused | PlaybackState::Stopped | PlaybackState::Unknown => {
                "media-playback-start-symbolic"
            }
        };
        let album_height = size.0.saturating_mul(4);
        let album_width = album_height.saturating_mul(16) / 9;

        let album_widget = self
            .album_art_path
            .as_ref()
            .map(|path| {
                icon::icon(icon::from_path(path.clone()))
                    .height(Length::Fixed(f32::from(album_height)))
                    .width(Length::Fixed(f32::from(album_width)))
                    .content_fit(ContentFit::Contain)
            })
            .unwrap_or_else(|| {
                icon::from_name("audio-x-generic-symbolic")
                    .size(album_height)
                    .icon()
                    .height(Length::Fixed(f32::from(album_height)))
                    .width(Length::Fixed(f32::from(album_width)))
                    .content_fit(ContentFit::Contain)
            });

        let controls = Row::new()
            .spacing(pad.0)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .push(
                button::icon(icon::from_name("media-skip-backward-symbolic").size(size.0 + 4))
                    .on_press(Message::PreviousTrack),
            )
            .push(
                button::icon(icon::from_name(transport_icon).size(size.0 + 4))
                    .on_press(Message::TogglePlayPause),
            )
            .push(
                button::icon(icon::from_name("media-skip-forward-symbolic").size(size.0 + 4))
                    .on_press(Message::NextTrack),
            );

        let media_info = column()
            .spacing(2)
            .align_x(cosmic::iced::Alignment::Center)
            .push(
                text(self.now_playing_title.as_str())
                    .size(size.0.saturating_sub(1))
                    .width(Length::Fill)
                    .align_x(cosmic::iced::alignment::Horizontal::Center)
                    .wrapping(Wrapping::WordOrGlyph),
            )
            .push(
                text(self.now_playing_artist.as_str())
                    .size(size.0.saturating_sub(3))
                    .width(Length::Fill)
                    .align_x(cosmic::iced::alignment::Horizontal::Center)
                    .wrapping(Wrapping::WordOrGlyph),
            );

        let content_list = column()
            .padding(12)
            .spacing(12)
            .align_x(cosmic::iced::Alignment::Center)
            .push(album_widget)
            .push(media_info)
            .push(controls);

        // Set the widget content list as the popup_container for the applet
        self.core.applet.popup_container(content_list).into()
    }
}

impl Window {
    fn has_active_media(&self) -> bool {
        !(self.playback_state == PlaybackState::Stopped
            && self.now_playing_title == "Nothing playing"
            && self.now_playing_artist.is_empty())
    }
}

fn style_with_optional_album_color(mut base: button::Style, color: Option<Color>) -> button::Style {
    if let Some(album) = color {
        let theme_base = match base.background {
            Some(Background::Color(c)) => c,
            _ => Color::from_rgb8(36, 38, 42),
        };

        let mixed = blend_color(theme_base, album, 0.36);
        let background = with_alpha(mixed, 0.64);
        let border = with_alpha(shift_color(album, -0.05), 0.82);

        let foreground = contrast_text_color(background);
        base.background = Some(Background::Color(background));
        base.border_width = base.border_width.max(1.0);
        base.border_color = border;
        base.text_color = Some(foreground);
        base.icon_color = Some(foreground);
    }
    base
}

fn with_alpha(mut color: Color, alpha: f32) -> Color {
    color.a = alpha.clamp(0.0, 1.0);
    color
}

fn blend_color(a: Color, b: Color, ratio: f32) -> Color {
    let t = ratio.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color {
        r: (a.r * inv) + (b.r * t),
        g: (a.g * inv) + (b.g * t),
        b: (a.b * inv) + (b.b * t),
        a: (a.a * inv) + (b.a * t),
    }
}

fn contrast_text_color(background: Color) -> Color {
    let luminance = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if luminance > 0.58 {
        Color::from_rgb8(17, 17, 17)
    } else {
        Color::WHITE
    }
}

fn shift_color(color: Color, amount: f32) -> Color {
    let adjust = |channel: f32| (channel + amount).clamp(0.0, 1.0);
    Color {
        r: adjust(color.r),
        g: adjust(color.g),
        b: adjust(color.b),
        a: color.a,
    }
}

fn dominant_album_color(path: Option<&PathBuf>) -> Option<Color> {
    let path = path?;
    let image = image::open(path).ok()?.to_rgba8();
    let thumb = image::imageops::thumbnail(&image, 64, 64);

    let mut buckets: HashMap<u16, (u32, u64, u64, u64)> = HashMap::new();
    for pixel in thumb.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 24 {
            continue;
        }

        // Group into coarse RGB buckets so we pick the most frequent color family.
        let key = ((u16::from(r >> 4)) << 8) | ((u16::from(g >> 4)) << 4) | u16::from(b >> 4);
        let entry = buckets.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.1 += u64::from(r);
        entry.2 += u64::from(g);
        entry.3 += u64::from(b);
    }

    let (_, (count, sum_r, sum_g, sum_b)) = buckets.into_iter().max_by_key(|(_, (count, ..))| *count)?;
    if count == 0 {
        return None;
    }

    let r = (sum_r / u64::from(count)) as u8;
    let g = (sum_g / u64::from(count)) as u8;
    let b = (sum_b / u64::from(count)) as u8;
    Some(Color::from_rgb8(r, g, b))
}
