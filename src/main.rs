use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, ToolbarView, Clamp, StyleManager};
use gtk4::{Align, Box as GtkBox, Button, Entry, Orientation, ScrolledWindow, Label, CssProvider, FileDialog, MenuButton, Popover, Switch, Image, Paned, ListBox, ListBoxRow};
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
    ToolUse(String, String),
    Error(String),
    Done,
    SessionsLoaded(Vec<String>),
    ClearChat,
}

#[derive(Deserialize, Debug)]
struct RawStream {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    role: Option<String>,
    content: Option<String>,
    status: Option<String>,
    error: Option<String>,
    tool_name: Option<String>,
    parameters: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct HistoryFile {
    messages: Option<Vec<HistoryMessage>>,
}

#[derive(Deserialize, Debug)]
struct HistoryMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    content: Option<serde_json::Value>,
    #[serde(rename = "toolCalls")]
    tool_calls: Option<Vec<HistoryToolCall>>,
}

#[derive(Deserialize, Debug)]
struct HistoryToolCall {
    name: Option<String>,
    args: Option<serde_json::Value>,
}

fn load_css() {
    let provider = CssProvider::new();
    provider.load_from_data(
        "
        .material-window { background-color: @window_bg_color; }
        .material-header { background: transparent; border-bottom: none; box-shadow: none; }
        .material-chat-view { background-color: transparent; }
        
        .sidebar {
            background-color: @view_bg_color;
            border-right: 1px solid @border_color;
        }
        .sidebar-item {
            padding: 12px 16px;
            font-size: 0.95em;
            color: @window_fg_color;
            border-radius: 8px;
            margin: 2px 8px;
        }
        .sidebar-item:hover { background-color: @card_shade_color; }
        
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
        .bot-icon { color: @accent_bg_color; margin-right: 16px; margin-top: 4px; }
        .system-bubble { color: @dim_label_color; font-style: italic; font-size: 0.9em; margin: 8px 0; }
        
        /* Artifact / Tool Call Card */
        .tool-card {
            background-color: @card_bg_color;
            border: 1px solid @card_shade_color;
            border-radius: 12px;
            padding: 12px;
            margin: 8px 0px;
            box-shadow: 0 1px 2px rgba(0,0,0,0.05);
        }
        .tool-header { font-weight: bold; color: @accent_color; margin-bottom: 4px; }
        .tool-desc { font-family: monospace; font-size: 0.9em; color: @dim_label_color; }

        .material-input-container { background: transparent; padding: 12px 24px 32px 24px; }
        .material-search-bar {
            background-color: @card_bg_color;
            border-radius: 32px;
            padding: 6px 6px 6px 16px;
            box-shadow: 0 4px 12px rgba(0,0,0,0.1);
            border: 1px solid @window_bg_color;
        }
        .material-entry { background: transparent; border: none; box-shadow: none; font-size: 1.1em; }
        .material-entry:focus { outline: none; box-shadow: none; }
        
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
        .material-send-btn:hover { background-image: image(rgba(255,255,255,0.1)); box-shadow: 0 4px 8px rgba(0,0,0,0.3); }
        .workspace-btn { border-radius: 18px; font-weight: bold; background-color: @card_bg_color; }
        .welcome-title { font-size: 2.5em; font-weight: bold; color: @window_fg_color; margin-top: 16px; }
        .welcome-subtitle { font-size: 1.2em; color: @dim_label_color; margin-top: 8px; }

        /* Accent Colors Variants */
        .color-swatch {
            min-width: 32px;
            min-height: 32px;
            border-radius: 50%;
            padding: 0;
            margin: 0 6px;
            border: 3px solid transparent;
            box-shadow: 0 2px 4px rgba(0,0,0,0.15);
        }
        .color-swatch:hover { opacity: 0.8; }
        .swatch-blue { background-color: #3584e4; }
        .swatch-green { background-color: #2ec27e; }
        .swatch-purple { background-color: #813d9c; }
        .swatch-selected { border-color: @window_fg_color; }

        .theme-blue { @define-color accent_color #3584e4; @define-color accent_bg_color #3584e4; @define-color accent_fg_color #ffffff; }
        .theme-green { @define-color accent_color #2ec27e; @define-color accent_bg_color #2ec27e; @define-color accent_fg_color #ffffff; }
        .theme-purple { @define-color accent_color #813d9c; @define-color accent_bg_color #813d9c; @define-color accent_fg_color #ffffff; }
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
    StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Gemini Extended")
        .default_width(1100)
        .default_height(800)
        .css_classes(["material-window"])
        .build();

    let paned = Paned::builder().orientation(Orientation::Horizontal).hexpand(true).vexpand(true).build();

    let current_dir = Rc::new(RefCell::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))));
    let yolo_mode = Rc::new(RefCell::new(false));
    let active_session: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    // --- SIDEBAR ---
    let sidebar_box = GtkBox::new(Orientation::Vertical, 0);
    sidebar_box.add_css_class("sidebar");
    sidebar_box.set_size_request(280, -1);
    let sidebar_header = HeaderBar::builder().show_end_title_buttons(false).build();
    sidebar_box.append(&sidebar_header);

    let new_chat_btn = Button::builder().label("New Chat").css_classes(["suggested-action", "pill"])
        .margin_top(12).margin_bottom(12).margin_start(16).margin_end(16).build();
    sidebar_box.append(&new_chat_btn);

    let sessions_scroll = ScrolledWindow::builder().hexpand(true).vexpand(true).build();
    let sessions_list = ListBox::builder().selection_mode(gtk4::SelectionMode::Single).css_classes(["navigation-sidebar"]).build();
    let loading_label = Label::builder().label("Loading sessions...").css_classes(["dim-label"]).margin_top(16).build();
    let loading_row = ListBoxRow::builder().child(&loading_label).activatable(false).selectable(false).build();
    sessions_list.append(&loading_row);
    sessions_scroll.set_child(Some(&sessions_list));
    sidebar_box.append(&sessions_scroll);
    paned.set_start_child(Some(&sidebar_box));

    // --- MAIN CONTENT ---
    let content_toolbar_view = ToolbarView::builder().hexpand(true).vexpand(true).build();
    let header_bar = HeaderBar::builder().css_classes(["material-header"]).build();
    content_toolbar_view.add_top_bar(&header_bar);

    // Workspace
    let workspace_box = GtkBox::new(Orientation::Horizontal, 6);
    workspace_box.set_halign(Align::Center);
    let dir_button = Button::builder().css_classes(["workspace-btn", "flat"]).tooltip_text("Change directory").build();
    let folder_name = current_dir.borrow().file_name().unwrap_or_default().to_string_lossy().to_string();
    let btn_content = GtkBox::new(Orientation::Horizontal, 6);
    let folder_icon = Image::from_icon_name("folder-open-symbolic");
    let dir_label = Label::builder().label(&folder_name).build();
    btn_content.append(&folder_icon); btn_content.append(&dir_label);
    dir_button.set_child(Some(&btn_content));
    workspace_box.append(&dir_button);
    header_bar.set_title_widget(Some(&workspace_box));

    let window_clone = window.clone();
    let current_dir_clone = current_dir.clone();
    let dir_label_clone = dir_label.clone();
    dir_button.connect_clicked(move |_| {
        let dialog = FileDialog::builder().title("Select Workspace").build();
        let cd_clone = current_dir_clone.clone();
        let dl_clone = dir_label_clone.clone();
        dialog.select_folder(Some(&window_clone), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(folder) = result {
                if let Some(path) = folder.path() {
                    *cd_clone.borrow_mut() = path.clone();
                    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    dl_clone.set_label(&name);
                }
            }
        });
    });

    // Settings
    let settings_menu = MenuButton::builder().icon_name("open-menu-symbolic").tooltip_text("Settings & Theme").build();
    let popover = Popover::builder().build();
    let popover_box = GtkBox::new(Orientation::Vertical, 12);
    popover_box.set_margin_start(16); popover_box.set_margin_end(16); popover_box.set_margin_top(16); popover_box.set_margin_bottom(16);

    let yolo_box = GtkBox::new(Orientation::Horizontal, 12);
    let yolo_label = Label::builder().label("<b>Auto-Approve (YOLO)</b>").use_markup(true).hexpand(true).xalign(0.0).build();
    let yolo_switch = Switch::new();
    let yolo_mode_clone = yolo_mode.clone();
    yolo_switch.connect_state_set(move |_, state| { *yolo_mode_clone.borrow_mut() = state; glib::Propagation::Proceed });
    yolo_box.append(&yolo_label); yolo_box.append(&yolo_switch);
    popover_box.append(&yolo_box);

    let theme_box = GtkBox::new(Orientation::Horizontal, 12);
    theme_box.set_margin_top(8);
    let theme_label = Label::builder().label("Accent Color").hexpand(true).xalign(0.0).build();
    theme_box.append(&theme_label);
    
    let btn_blue = Button::builder().css_classes(["color-swatch", "swatch-blue", "swatch-selected"]).build();
    let btn_green = Button::builder().css_classes(["color-swatch", "swatch-green"]).build();
    let btn_purple = Button::builder().css_classes(["color-swatch", "swatch-purple"]).build();
    
    let window_theme_clone = window.clone();
    let btn_blue_clone = btn_blue.clone();
    let btn_green_clone = btn_green.clone();
    let btn_purple_clone = btn_purple.clone();
    btn_blue.connect_clicked(move |btn| {
        window_theme_clone.set_css_classes(&["material-window", "theme-blue"]);
        btn.add_css_class("swatch-selected"); btn_green_clone.remove_css_class("swatch-selected"); btn_purple_clone.remove_css_class("swatch-selected");
    });
    
    let window_theme_clone = window.clone();
    let btn_blue_clone = btn_blue.clone();
    let btn_green_clone = btn_green.clone();
    let btn_purple_clone = btn_purple.clone();
    btn_green.connect_clicked(move |btn| {
        window_theme_clone.set_css_classes(&["material-window", "theme-green"]);
        btn.add_css_class("swatch-selected"); btn_blue_clone.remove_css_class("swatch-selected"); btn_purple_clone.remove_css_class("swatch-selected");
    });
    
    let window_theme_clone = window.clone();
    let btn_blue_clone = btn_blue.clone();
    let btn_green_clone = btn_green.clone();
    let btn_purple_clone = btn_purple.clone();
    btn_purple.connect_clicked(move |btn| {
        window_theme_clone.set_css_classes(&["material-window", "theme-purple"]);
        btn.add_css_class("swatch-selected"); btn_blue_clone.remove_css_class("swatch-selected"); btn_green_clone.remove_css_class("swatch-selected");
    });
    
    theme_box.append(&btn_blue); theme_box.append(&btn_green); theme_box.append(&btn_purple);
    popover_box.append(&theme_box);
    popover.set_child(Some(&popover_box));
    settings_menu.set_popover(Some(&popover));
    header_bar.pack_end(&settings_menu);

    // Chat View
    let scroll_window = ScrolledWindow::builder().hexpand(true).vexpand(true).hscrollbar_policy(gtk4::PolicyType::Never).css_classes(["material-chat-view"]).build();
    let clamp_chat = Clamp::builder().maximum_size(850).build();
    let chat_box = GtkBox::new(Orientation::Vertical, 12);
    chat_box.set_margin_top(32); chat_box.set_margin_bottom(32); chat_box.set_margin_start(24); chat_box.set_margin_end(24);
    
    // Welcome
    let welcome_box = GtkBox::new(Orientation::Vertical, 0);
    welcome_box.set_valign(Align::Center); welcome_box.set_vexpand(true);
    let welcome_icon = Image::builder().icon_name("weather-clear-night-symbolic").pixel_size(80).build();
    welcome_icon.add_css_class("accent");
    let welcome_title = Label::builder().label("Hello, I'm Gemini").css_classes(["welcome-title"]).build();
    let welcome_subtitle = Label::builder().label("How can I help you today?").css_classes(["welcome-subtitle"]).build();
    welcome_box.append(&welcome_icon); welcome_box.append(&welcome_title); welcome_box.append(&welcome_subtitle);
    chat_box.append(&welcome_box);

    clamp_chat.set_child(Some(&chat_box));
    scroll_window.set_child(Some(&clamp_chat));
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&scroll_window);

    // Input Area
    let input_container = GtkBox::new(Orientation::Vertical, 0);
    input_container.add_css_class("material-input-container");
    let clamp_input = Clamp::builder().maximum_size(850).build();
    let input_box = GtkBox::new(Orientation::Horizontal, 0);
    input_box.add_css_class("material-search-bar");
    
    let prompt_entry = Entry::builder().placeholder_text("Ask Gemini...").hexpand(true).css_classes(["flat", "material-entry"]).build();
    let send_button = Button::builder().icon_name("go-up-symbolic").css_classes(["material-send-btn"]).tooltip_text("Send prompt").build();

    input_box.append(&prompt_entry); input_box.append(&send_button);
    clamp_input.set_child(Some(&input_box)); input_container.append(&clamp_input); main_box.append(&input_container);

    content_toolbar_view.set_content(Some(&main_box)); paned.set_end_child(Some(&content_toolbar_view));
    paned.set_position(280); window.set_content(Some(&paned));

    // --- IPC ---
    let (ui_sender, mut ui_receiver) = mpsc::channel::<UiMessage>(100);
    // Request channel: Prompt, CWD, YOLO, SessionID, is_history_request
    let (async_sender, mut async_receiver) = mpsc::channel::<(String, PathBuf, bool, Option<String>, bool)>(32);

    let chat_box_clone = chat_box.clone();
    let scroll_window_clone = scroll_window.clone();
    let sessions_list_clone = sessions_list.clone();
    let loading_row_clone = loading_row.clone();
    let welcome_box_clone = welcome_box.clone();
    
    let current_bot_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let current_system_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));

    // Clear Chat Closure
    let clear_chat = Rc::new({
        let chat_box_for_clear = chat_box.clone();
        let welcome_box_for_clear = welcome_box.clone();
        move || {
            let mut child = chat_box_for_clear.first_child();
            while let Some(c) = child {
                let next = c.next_sibling();
                if c != welcome_box_for_clear { chat_box_for_clear.remove(&c); }
                child = next;
            }
        }
    });

    let active_session_nc = active_session.clone();
    let clear_chat_nc = clear_chat.clone();
    let welcome_box_nc = welcome_box.clone();
    let sessions_list_deselect = sessions_list.clone();
    new_chat_btn.connect_clicked(move |_| {
        *active_session_nc.borrow_mut() = None;
        sessions_list_deselect.unselect_all();
        clear_chat_nc();
        welcome_box_nc.set_visible(true);
    });

    let active_session_row = active_session.clone();
    let clear_chat_row = clear_chat.clone();
    let welcome_box_row = welcome_box.clone();
    let async_sender_row = async_sender.clone();
    let current_dir_row = current_dir.clone();
    
    sessions_list.connect_row_activated(move |_, row| {
        if let Some(child) = row.first_child() {
            if let Ok(label) = child.downcast::<Label>() {
                let text = label.label().to_string();
                if let Some(start) = text.rfind('[') {
                    if let Some(end) = text.rfind(']') {
                        let id = text[start+1..end].to_string();
                        *active_session_row.borrow_mut() = Some(id.clone());
                        clear_chat_row();
                        welcome_box_row.set_visible(false);
                        
                        let sender = async_sender_row.clone();
                        let cwd = current_dir_row.borrow().clone();
                        glib::spawn_future_local(async move {
                            // Send a request to Tokio to just load and parse this history file!
                            if let Err(e) = sender.send(("".to_string(), cwd, false, Some(id), true)).await {
                                eprintln!("Failed to request history: {}", e);
                            }
                        });
                    }
                }
            }
        }
    });

    // GTK UI Thread
    glib::spawn_future_local(async move {
        while let Some(msg) = ui_receiver.recv().await {
            match msg {
                UiMessage::SessionsLoaded(sessions) => {
                    sessions_list_clone.remove(&loading_row_clone);
                    if sessions.is_empty() {
                        let lbl = Label::builder().label("No recent sessions found.").css_classes(["dim-label"]).margin_top(16).build();
                        let row = ListBoxRow::builder().child(&lbl).activatable(false).selectable(false).build();
                        sessions_list_clone.append(&row);
                    } else {
                        for s in sessions {
                            let label = Label::builder().label(&s).xalign(0.0).css_classes(["sidebar-item"]).ellipsize(gtk4::pango::EllipsizeMode::End).build();
                            let row = ListBoxRow::builder().child(&label).build();
                            sessions_list_clone.append(&row);
                        }
                    }
                }
                UiMessage::ClearChat => {
                    let mut child = chat_box_clone.first_child();
                    while let Some(c) = child {
                        let next = c.next_sibling();
                        if c != welcome_box_clone { chat_box_clone.remove(&c); }
                        child = next;
                    }
                }
                UiMessage::NewUserMessage(m) => {
                    welcome_box_clone.set_visible(false);
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
                UiMessage::ToolUse(name, desc) => {
                    *current_bot_label.borrow_mut() = None;
                    let tool_card = GtkBox::new(Orientation::Vertical, 4);
                    tool_card.add_css_class("tool-card");
                    let header = GtkBox::new(Orientation::Horizontal, 8);
                    let icon = Image::from_icon_name("applications-engineering-symbolic");
                    let title = Label::builder().label(&format!("🛠️ Executed Tool: {}", name)).xalign(0.0).css_classes(["tool-header"]).build();
                    header.append(&icon); header.append(&title);
                    let body = Label::builder().label(&desc).wrap(true).xalign(0.0).css_classes(["tool-desc"]).build();
                    tool_card.append(&header); tool_card.append(&body);
                    chat_box_clone.append(&tool_card);
                }
                UiMessage::EngineOutput(m) => {
                    let mut bot_opt = current_bot_label.borrow_mut();
                    if bot_opt.is_none() {
                        let container = GtkBox::builder().css_classes(["bot-bubble"]).orientation(Orientation::Horizontal).halign(Align::Fill).build();
                        let bot_icon = Image::builder().icon_name("weather-clear-night-symbolic").pixel_size(24).css_classes(["bot-icon"]).valign(Align::Start).build();
                        let label = Label::builder().label(&m).wrap(true).xalign(0.0).selectable(true).hexpand(true).build();
                        container.append(&bot_icon); container.append(&label);
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

    // Tokio Async Worker
    let ui_sender_clone = ui_sender.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();

        rt.block_on(async move {
            if let Ok(output) = Command::new("gemini").arg("--list-sessions").output().await {
                let out_str = String::from_utf8_lossy(&output.stdout);
                let mut sessions = vec![];
                for line in out_str.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with(char::is_numeric) { sessions.push(trimmed.to_string()); }
                }
                ui_sender_clone.send(UiMessage::SessionsLoaded(sessions)).await.unwrap();
            }

            while let Some((prompt, cwd, yolo, session_id, is_history)) = async_receiver.recv().await {
                
                // HISTORY RESTORE LOGIC
                if is_history {
                    if let Some(uuid) = session_id {
                        let short_uuid = uuid.split('-').next().unwrap_or(&uuid);
                        if let Ok(output) = Command::new("find").arg(dirs::home_dir().unwrap().join(".gemini")).arg("-type").arg("f").arg("-name").arg(format!("*{}*.json", short_uuid)).output().await {
                            let path_str = String::from_utf8_lossy(&output.stdout);
                            if let Some(first_path) = path_str.lines().next() {
                                if let Ok(contents) = tokio::fs::read_to_string(first_path).await {
                                    if let Ok(history) = serde_json::from_str::<HistoryFile>(&contents) {
                                        if let Some(msgs) = history.messages {
                                            for msg in msgs {
                                                if msg.msg_type.as_deref() == Some("user") {
                                                    if let Some(content) = msg.content {
                                                        if let Some(arr) = content.as_array() {
                                                            if let Some(obj) = arr.get(0) {
                                                                if let Some(txt) = obj.get("text").and_then(|t| t.as_str()) {
                                                                    ui_sender_clone.send(UiMessage::NewUserMessage(txt.to_string())).await.unwrap();
                                                                }
                                                            }
                                                        }
                                                    }
                                                } else if msg.msg_type.as_deref() == Some("gemini") {
                                                    if let Some(content) = msg.content {
                                                        if let Some(txt) = content.as_str() {
                                                            ui_sender_clone.send(UiMessage::EngineOutput(txt.to_string())).await.unwrap();
                                                            ui_sender_clone.send(UiMessage::Done).await.unwrap(); // flush the bubble
                                                        }
                                                    }
                                                    if let Some(calls) = msg.tool_calls {
                                                        for call in calls {
                                                            let name = call.name.unwrap_or_default();
                                                            let desc = call.args.map(|p| p.to_string()).unwrap_or_default();
                                                            ui_sender_clone.send(UiMessage::ToolUse(name, desc)).await.unwrap();
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ui_sender_clone.send(UiMessage::SystemMessage("Resumed session.".to_string())).await.unwrap();
                    }
                    continue; // History loaded, don't spawn gemini yet!
                }

                // NORMAL PROMPT EXECUTION
                ui_sender_clone.send(UiMessage::SystemMessage("Thinking...".to_string())).await.unwrap();

                let mut cmd = Command::new("gemini");
                cmd.current_dir(&cwd).arg("-p").arg(&prompt).arg("-o").arg("stream-json")
                   .stdout(Stdio::piped()).stderr(Stdio::piped());

                if yolo { cmd.arg("-y"); }
                if let Some(sid) = session_id { cmd.arg("-r").arg(&sid); }

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
                            if let Some(txt) = parsed.content { ui_sender_clone.send(UiMessage::EngineOutput(txt)).await.unwrap(); }
                        } else if parsed.msg_type.as_deref() == Some("tool_use") {
                            let name = parsed.tool_name.unwrap_or_else(|| "Unknown".to_string());
                            let desc = parsed.parameters.map(|p| p.to_string()).unwrap_or_default();
                            ui_sender_clone.send(UiMessage::ToolUse(name, desc)).await.unwrap();
                        } else if parsed.msg_type.as_deref() == Some("result") && parsed.status.as_deref() == Some("error") {
                            if let Some(err) = parsed.error { ui_sender_clone.send(UiMessage::Error(err)).await.unwrap(); }
                        }
                    }
                }
                
                ui_sender_clone.send(UiMessage::Done).await.unwrap();
            }
        });
    });

    let async_sender_clone = async_sender.clone();
    let prompt_entry_clone = prompt_entry.clone();
    let ui_sender_input = ui_sender.clone();
    let current_dir_action = current_dir.clone();
    let yolo_action = yolo_mode.clone();
    let active_session_action = active_session.clone();

    let send_action = move || {
        let text = prompt_entry_clone.text().to_string();
        if !text.trim().is_empty() {
            prompt_entry_clone.set_text("");
            let async_sender_local = async_sender_clone.clone();
            let ui_sender_local = ui_sender_input.clone();
            let cwd = current_dir_action.borrow().clone();
            let yolo = *yolo_action.borrow();
            let session = active_session_action.borrow().clone();
            
            glib::spawn_future_local(async move {
                ui_sender_local.send(UiMessage::NewUserMessage(text.clone())).await.unwrap();
                if let Err(e) = async_sender_local.send((text, cwd, yolo, session, false)).await {
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
    let app = Application::builder().application_id("com.github.redminote11tech.GeminiExtended").build();
    app.connect_activate(move |app| { build_ui(app); });
    app.run()
}