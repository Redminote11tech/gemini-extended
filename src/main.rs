use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, ToolbarView, Clamp, StyleManager};
use gtk4::{Align, Box as GtkBox, Button, Entry, Orientation, ScrolledWindow, Label, CssProvider, FileDialog, MenuButton, Popover, Switch, Image};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use serde::Deserialize;
use std::rc::Rc;
use std::cell::RefCell;
use std::path::PathBuf;

pub enum UiMessage {
    NewUserMessage(String),
    SystemMessage(String),
    EngineOutput(String),
    Error(String),
    Done,
}

#[derive(Deserialize, Debug)]
struct RawStream {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    role: Option<String>,
    content: Option<String>,
    status: Option<String>,
    error: Option<String>,
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        .material-window {
            background-color: @window_bg_color;
        }
        .material-header {
            background: transparent;
            border-bottom: none;
            box-shadow: none;
        }
        .material-chat-view {
            background-color: transparent;
        }
        
        /* Chat Bubbles */
        .user-bubble { 
            background-color: @accent_bg_color; 
            color: @accent_fg_color; 
            border-radius: 20px 20px 4px 20px; 
            padding: 12px 18px; 
            margin: 16px 0px 8px 0px; 
            font-size: 1.05em;
            box-shadow: 0 1px 3px rgba(0,0,0,0.15);
        }
        .bot-bubble { 
            background-color: transparent; 
            color: @window_fg_color; 
            padding: 4px 0px; 
            margin: 8px 0px 16px 0px; 
            font-size: 1.05em;
            line-height: 1.6;
        }
        .bot-icon {
            color: @accent_bg_color;
            margin-right: 16px;
            margin-top: 4px;
        }
        .system-bubble { 
            color: @dim_label_color; 
            font-style: italic; 
            font-size: 0.9em;
            margin: 8px 0;
        }

        /* Material Input Bar */
        .material-input-container {
            background: transparent;
            padding: 12px 24px 32px 24px;
        }
        .material-search-bar {
            background-color: @card_bg_color;
            border-radius: 32px;
            padding: 6px 6px 6px 16px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.1);
            border: 1px solid @window_bg_color;
        }
        .material-entry {
            background: transparent;
            border: none;
            box-shadow: none;
            font-size: 1.1em;
        }
        .material-entry:focus {
            outline: none;
            box-shadow: none;
        }
        .material-send-btn {
            border-radius: 50%;
            background-color: @accent_bg_color;
            color: @accent_fg_color;
            min-width: 44px;
            min-height: 44px;
            padding: 0;
            margin-left: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.2);
            transition: all 0.2s ease;
        }
        .material-send-btn:hover {
            background-image: image(rgba(255,255,255,0.1));
            box-shadow: 0 4px 8px rgba(0,0,0,0.3);
        }
        .workspace-btn {
            border-radius: 18px;
            font-weight: bold;
            background-color: @card_bg_color;
        }
        .welcome-title {
            font-size: 2.5em;
            font-weight: bold;
            color: @window_fg_color;
            margin-top: 16px;
        }
        .welcome-subtitle {
            font-size: 1.2em;
            color: @dim_label_color;
            margin-top: 8px;
        }
        "
    );
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().expect("Could not connect to a display."),
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(app: &Application) {
    load_css();

    // Default System Theme (Follow OS settings automatically)
    StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Gemini Extended")
        .default_width(950)
        .default_height(800)
        .css_classes(["material-window"])
        .build();

    let toolbar_view = ToolbarView::builder().build();
    let header_bar = HeaderBar::builder().css_classes(["material-header"]).build();
    toolbar_view.add_top_bar(&header_bar);

    // --- State Variables ---
    let current_dir = Rc::new(RefCell::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))));
    let yolo_mode = Rc::new(RefCell::new(false));

    // --- Header: Workspace Selection ---
    let workspace_box = GtkBox::new(Orientation::Horizontal, 6);
    workspace_box.set_halign(Align::Center);
    
    let dir_button = Button::builder()
        .css_classes(["workspace-btn", "flat"])
        .tooltip_text("Change contained working directory")
        .build();
    
    // Initial folder name setup
    let folder_name = current_dir.borrow().file_name().unwrap_or_default().to_string_lossy().to_string();
    let btn_content = GtkBox::new(Orientation::Horizontal, 6);
    let folder_icon = Image::from_icon_name("folder-open-symbolic");
    let dir_label = Label::builder().label(&folder_name).build();
    btn_content.append(&folder_icon);
    btn_content.append(&dir_label);
    dir_button.set_child(Some(&btn_content));
    
    workspace_box.append(&dir_button);
    header_bar.set_title_widget(Some(&workspace_box));

    let window_clone = window.clone();
    let current_dir_clone = current_dir.clone();
    let dir_label_clone = dir_label.clone();
    dir_button.connect_clicked(move |_| {
        let dialog = FileDialog::builder().title("Select Workspace Directory").build();
        let current_dir_clone = current_dir_clone.clone();
        let dir_label_clone = dir_label_clone.clone();
        dialog.select_folder(Some(&window_clone), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(folder) = result {
                if let Some(path) = folder.path() {
                    *current_dir_clone.borrow_mut() = path.clone();
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    dir_label_clone.set_label(&*name);
                }
            }
        });
    });

    // --- Header: Settings Menu (YOLO Mode Only, System Theme is auto) ---
    let settings_menu = MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Settings & Permissions")
        .build();
    
    let popover = Popover::builder().build();
    let popover_box = GtkBox::new(Orientation::Vertical, 12);
    popover_box.set_margin_start(16);
    popover_box.set_margin_end(16);
    popover_box.set_margin_top(16);
    popover_box.set_margin_bottom(16);

    // YOLO Mode Toggle
    let yolo_box = GtkBox::new(Orientation::Horizontal, 12);
    let yolo_label = Label::builder().label("<b>Auto-Approve (YOLO)</b>").use_markup(true).hexpand(true).xalign(0.0).build();
    let yolo_switch = Switch::new();
    let yolo_mode_clone = yolo_mode.clone();
    yolo_switch.connect_state_set(move |_, state| {
        *yolo_mode_clone.borrow_mut() = state;
        glib::Propagation::Proceed
    });
    yolo_box.append(&yolo_label);
    yolo_box.append(&yolo_switch);

    popover_box.append(&yolo_box);
    popover.set_child(Some(&popover_box));
    settings_menu.set_popover(Some(&popover));
    header_bar.pack_end(&settings_menu);

    // --- Chat View ---
    let scroll_window = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .css_classes(["material-chat-view"])
        .build();

    let clamp_chat = Clamp::builder().maximum_size(850).build();
    
    let chat_box = GtkBox::new(Orientation::Vertical, 12);
    chat_box.set_margin_top(32);
    chat_box.set_margin_bottom(32);
    chat_box.set_margin_start(24);
    chat_box.set_margin_end(24);
    
    // Welcome Screen / Empty State
    let welcome_box = GtkBox::new(Orientation::Vertical, 0);
    welcome_box.set_valign(Align::Center);
    welcome_box.set_vexpand(true);
    
    let welcome_icon = Image::builder().icon_name("weather-clear-night-symbolic").pixel_size(80).build();
    welcome_icon.add_css_class("accent");
    
    let welcome_title = Label::builder().label("Hello, I'm Gemini").css_classes(["welcome-title"]).build();
    let welcome_subtitle = Label::builder().label("How can I help you today?").css_classes(["welcome-subtitle"]).build();
    
    welcome_box.append(&welcome_icon);
    welcome_box.append(&welcome_title);
    welcome_box.append(&welcome_subtitle);
    
    chat_box.append(&welcome_box);

    clamp_chat.set_child(Some(&chat_box));
    scroll_window.set_child(Some(&clamp_chat));
    
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&scroll_window);

    // --- Input Area (Material Search Bar) ---
    let input_container = GtkBox::new(Orientation::Vertical, 0);
    input_container.add_css_class("material-input-container");

    let clamp_input = Clamp::builder().maximum_size(850).build();
    
    let input_box = GtkBox::new(Orientation::Horizontal, 0);
    input_box.add_css_class("material-search-bar");
    
    let prompt_entry = Entry::builder()
        .placeholder_text("Ask Gemini to write code, analyze files, or fix bugs...")
        .hexpand(true)
        .css_classes(["flat", "material-entry"])
        .build();
    
    let send_button = Button::builder()
        .icon_name("send-symbolic")
        .css_classes(["material-send-btn"])
        .tooltip_text("Send prompt")
        .build();

    input_box.append(&prompt_entry);
    input_box.append(&send_button);
    clamp_input.set_child(Some(&input_box));
    input_container.append(&clamp_input);
    main_box.append(&input_container);

    toolbar_view.set_content(Some(&main_box));
    window.set_content(Some(&toolbar_view));

    // --- Channels & IPC ---
    let (ui_sender, mut ui_receiver) = mpsc::channel::<UiMessage>(100);
    let (async_sender, mut async_receiver) = mpsc::channel::<(String, PathBuf, bool)>(32);

    let chat_box_clone = chat_box.clone();
    let scroll_window_clone = scroll_window.clone();
    
    let current_bot_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let current_system_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let welcome_box_ref = Rc::new(RefCell::new(Some(welcome_box)));

    // --- UI Update Loop (GTK Thread) ---
    glib::spawn_future_local(async move {
        while let Some(msg) = ui_receiver.recv().await {
            match msg {
                UiMessage::NewUserMessage(m) => {
                    // Hide welcome screen on first message
                    if let Some(wb) = welcome_box_ref.borrow_mut().take() {
                        chat_box_clone.remove(&wb);
                    }

                    *current_bot_label.borrow_mut() = None;
                    *current_system_label.borrow_mut() = None;

                    let label = Label::builder().label(&m).wrap(true).xalign(0.0).selectable(true).build();
                    let bubble = GtkBox::builder().css_classes(["user-bubble"]).halign(Align::End).build();
                    bubble.append(&label);
                    chat_box_clone.append(&bubble);
                }
                UiMessage::SystemMessage(m) => {
                    let mut sys_opt = current_system_label.borrow_mut();
                    if let Some(lbl) = sys_opt.as_ref() {
                        lbl.set_label(&m);
                    } else {
                        let label = Label::builder().label(&m).wrap(true).xalign(0.5).css_classes(["system-bubble"]).build();
                        chat_box_clone.append(&label);
                        *sys_opt = Some(label);
                    }
                }
                UiMessage::EngineOutput(m) => {
                    let mut bot_opt = current_bot_label.borrow_mut();
                    if bot_opt.is_none() {
                        let container = GtkBox::builder().css_classes(["bot-bubble"]).orientation(Orientation::Horizontal).halign(Align::Fill).build();
                        
                        let bot_icon = Image::builder().icon_name("weather-clear-night-symbolic").pixel_size(24).css_classes(["bot-icon"]).valign(Align::Start).build();
                        let label = Label::builder().label(&m).wrap(true).xalign(0.0).selectable(true).hexpand(true).build();
                        
                        container.append(&bot_icon);
                        container.append(&label);
                        
                        chat_box_clone.append(&container);
                        *bot_opt = Some(label);
                    } else {
                        let lbl = bot_opt.as_ref().unwrap();
                        let old = lbl.label();
                        lbl.set_label(&format!("{}{}", old, m));
                    }
                }
                UiMessage::Error(m) => {
                    let label = Label::builder().label(&format!("Error: {}", m)).wrap(true).xalign(0.0).css_classes(["error"]).build();
                    chat_box_clone.append(&label);
                }
                UiMessage::Done => {
                    if let Some(lbl) = current_system_label.borrow_mut().take() {
                        chat_box_clone.remove(&lbl);
                    }
                }
            }

            let adjustment = scroll_window_clone.vadjustment();
            adjustment.set_value(adjustment.upper());
        }
    });

    // --- Tokio Async Worker Loop ---
    let ui_sender_clone = ui_sender.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            while let Some((prompt, cwd, yolo)) = async_receiver.recv().await {
                ui_sender_clone.send(UiMessage::SystemMessage("Thinking...".to_string())).await.unwrap();

                let mut cmd = Command::new("gemini");
                cmd.current_dir(&cwd)
                   .arg("-p")
                   .arg(&prompt)
                   .arg("-o")
                   .arg("stream-json")
                   .stdout(Stdio::piped())
                   .stderr(Stdio::piped());

                if yolo {
                    cmd.arg("-y");
                }

                let mut child = match cmd.spawn() {
                    Ok(child) => child,
                    Err(e) => {
                        ui_sender_clone.send(UiMessage::Error(format!("Failed to spawn CLI: {}", e))).await.unwrap();
                        ui_sender_clone.send(UiMessage::Done).await.unwrap();
                        continue;
                    }
                };

                let stdout = child.stdout.take().expect("Failed to grab stdout");
                let mut reader = BufReader::new(stdout).lines();

                while let Ok(Some(line)) = reader.next_line().await {
                    if let Ok(parsed) = serde_json::from_str::<RawStream>(&line) {
                        if parsed.msg_type.as_deref() == Some("message") && parsed.role.as_deref() == Some("assistant") {
                            if let Some(txt) = parsed.content {
                                ui_sender_clone.send(UiMessage::EngineOutput(txt)).await.unwrap();
                            }
                        } else if parsed.msg_type.as_deref() == Some("result") && parsed.status.as_deref() == Some("error") {
                            if let Some(err) = parsed.error {
                                ui_sender_clone.send(UiMessage::Error(err)).await.unwrap();
                            }
                        }
                    }
                }
                
                ui_sender_clone.send(UiMessage::Done).await.unwrap();
            }
        });
    });

    // --- Input Actions ---
    let async_sender_clone = async_sender.clone();
    let prompt_entry_clone = prompt_entry.clone();
    let ui_sender_input = ui_sender.clone();
    let current_dir_action = current_dir.clone();
    let yolo_action = yolo_mode.clone();

    let send_action = move || {
        let text = prompt_entry_clone.text().to_string();
        if !text.trim().is_empty() {
            prompt_entry_clone.set_text("");
            
            let async_sender_local = async_sender_clone.clone();
            let ui_sender_local = ui_sender_input.clone();
            let cwd = current_dir_action.borrow().clone();
            let yolo = *yolo_action.borrow();
            
            glib::spawn_future_local(async move {
                ui_sender_local.send(UiMessage::NewUserMessage(text.clone())).await.unwrap();
                if let Err(e) = async_sender_local.send((text, cwd, yolo)).await {
                    eprintln!("Failed to send to async runtime: {}", e);
                }
            });
        }
    };

    let send_action_clone = send_action.clone();
    send_button.connect_clicked(move |_| send_action_clone());
    prompt_entry.connect_activate(move |_| send_action());

    window.present();
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .application_id("com.github.redminote11tech.GeminiExtended")
        .build();

    app.connect_activate(move |app| {
        build_ui(app);
    });

    app.run()
}