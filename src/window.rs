// Mandatory COSMIC imports
use std::time::Duration;

use cosmic::app::Core;
use cosmic::iced::{
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
use cosmic::widget::{button, icon, list_column, settings, text, toggler, Row};

// Every COSMIC Application and Applet MUST have an ID
const ID: &str = "com.example.BasicApplet";

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
    is_enabled: bool,
    now_playing_text: String,
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,         // Mandatory for open and close the applet
    PopupClosed(Id),     // Mandatory for the applet to know if it's been closed
    EnableDisable(bool), // Our custom message to update the isEnabled field on the model
    NowPlayingChanged(String),
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
        let window = Window {
            core,                 // Set the incoming core
            is_enabled: false,    // Set out isEnabled field to false to start disabled
            now_playing_text: now_playing(),
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
                        .max_width(372.0)
                        .min_width(300.0)
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
            Message::EnableDisable(is_enabled) => self.is_enabled = is_enabled,
            Message::NowPlayingChanged(text) => self.now_playing_text = text,
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
                                if last_sent != "Nothing playing" {
                                    last_sent = "Nothing playing".to_string();
                                    while output
                                        .try_send(Message::NowPlayingChanged(last_sent.clone()))
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
                        if current != last_sent {
                            last_sent = current.clone();
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

                                    if text != last_sent {
                                        last_sent = text.clone();
                                        while output
                                            .try_send(Message::NowPlayingChanged(text.clone()))
                                            .is_err()
                                        {
                                            std::thread::sleep(Duration::from_millis(10));
                                        }
                                    }
                                }
                                Ok(MprisEvent::Playing)
                                | Ok(MprisEvent::Paused)
                                | Ok(MprisEvent::Stopped) => {
                                    let text = now_playing_from_player(&player);

                                    if text != last_sent {
                                        last_sent = text.clone();
                                        while output
                                            .try_send(Message::NowPlayingChanged(text.clone()))
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
        let size = self.core.applet.suggested_size(true);
        let pad = self.core.applet.suggested_padding(true);

        let row_content = Row::new()
            .spacing(pad.0)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .push(icon::from_name("audio-x-generic-symbolic"))
            .push(
                text(self.now_playing_text.as_str())
                    .size(size.0)
                    .width(Length::Fixed(300.0))
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
            );

        let content = button::custom(row_content)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .on_press(Message::TogglePopup);

        self.core
            .applet
            .autosize_window(content)
            .into()
    }

    // The actual GUI window for the applet. It's a popup.
    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        // A text box to show if we've enabled or disabled anything in the model
        let content_list = list_column()
            .padding(5)
            .spacing(0)
            .add(settings::item(
                "Is this enabled?",
                text(if self.is_enabled {
                    "It is enabled!"
                } else {
                    "It's not enabled!"
                }),
            ))
            .add(settings::item(
                "Enable/Disable",
                toggler(self.is_enabled).on_toggle(Message::EnableDisable),
            ));

        // Set the widget content list as the popup_container for the applet
        self.core.applet.popup_container(content_list).into()
    }  
}

fn now_playing() -> String {
    let finder = PlayerFinder::new();

    if let Ok(finder) = finder {
        if let Ok(player) = finder.find_active() {
            return now_playing_from_player(&player);
        }
    }

    "Nothing playing".to_string()
}

fn now_playing_from_player(player: &mpris::Player) -> String {
    if let Ok(meta) = player.get_metadata() {
        let title = meta.title().unwrap_or("Unknown");
        let artist = meta
            .artists()
            .and_then(|a| a.first().copied())
            .unwrap_or("Unknown");

        return format!("{} - {}", title, artist);
    }

    "Nothing playing".to_string()
}