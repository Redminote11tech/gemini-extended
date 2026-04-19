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
        .chat-view {
            background-color: @view_bg_color;
        }
        .user-bubble { 
            background-color: @accent_bg_color; 
            color: @accent_fg_color; 
            border-radius: 18px 18px 0px 18px; 
            padding: 10px 16px; 
            margin-bottom: 8px; 
            box-shadow: 0 1px 2px rgba(0,0,0,0.1);
        }
        .bot-bubble { 
            background-color: @card_bg_color; 
            color: @card_fg_color; 
            border-radius: 18px 18px 18px 0px; 
            padding: 10px 16px; 
            margin-bottom: 8px; 
            border: 1px solid @card_shade_color;
            box-shadow: 0 1px 2px rgba(0,0,0,0.05);
        }
        .system-bubble { 
            color: @dim_label_color; 
            font-style: italic; 
            font-size: 0.9em;
            margin-bottom: 6px;
            margin-top: 6px;
        }
        .input-bar {
            background-color: @window_bg_color;
            border-top: 1px solid @border_color;
            padding: 12px 24px 24px 24px;
        }
        .pill-entry {
            border-radius: 24px;
            padding: 4px 12px;
        }
        .send-btn {
            border-radius: 50%;
            min-width: 36px;
            min-height: 36px;
            padding: 0;
        }
        .workspace-btn {
            border-radius: 18px;
            font-weight: bold;
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

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Gemini Extended")
        .default_width(900)
        .default_height(750)
        .build();

    let toolbar_view = ToolbarView::builder().build();
    let header_bar = HeaderBar::builder().build();
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
                    dir_label_clone.set_label(&name);
                }
            }
        });
    });

    // --- Header: Settings Menu ---
    let settings_menu = MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Settings & Permissions")
        .build();
    
    let popover = Popover::builder().build();
    let popover_box = GtkBox::new(Orientation::Vertical, 12);
    popover_box.set_margin_start(12);
    popover_box.set_margin_end(12);
    popover_box.set_margin_top(12);
    popover_box.set_margin_bottom(12);

    // Dark Mode Toggle
    let theme_box = GtkBox::new(Orientation::Horizontal, 12);
    let theme_label = Label::builder().label("Dark Mode").hexpand(true).xalign(0.0).build();
    let theme_switch = Switch::new();
    theme_switch.set_active(StyleManager::default().is_dark());
    theme_switch.connect_state_set(|_, state| {
        let manager = StyleManager::default();
        if state {
            manager.set_color_scheme(adw::ColorScheme::ForceDark);
        } else {
            manager.set_color_scheme(adw::ColorScheme::ForceLight);
        }
        glib::Propagation::Proceed
    });
    theme_box.append(&theme_label);
    theme_box.append(&theme_switch);

    // YOLO Mode (Permissions) Toggle
    let yolo_box = GtkBox::new(Orientation::Horizontal, 12);
    let yolo_label = Label::builder().label("Auto-Approve (YOLO)").hexpand(true).xalign(0.0).build();
    let yolo_switch = Switch::new();
    let yolo_mode_clone = yolo_mode.clone();
    yolo_switch.connect_state_set(move |_, state| {
        *yolo_mode_clone.borrow_mut() = state;
        glib::Propagation::Proceed
    });
    yolo_box.append(&yolo_label);
    yolo_box.append(&yolo_switch);

    popover_box.append(&theme_box);
    popover_box.append(&yolo_box);
    popover.set_child(Some(&popover_box));
    settings_menu.set_popover(Some(&popover));
    header_bar.pack_end(&settings_menu);

    // --- Chat View ---
    let scroll_window = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .css_classes(["chat-view"])
        .build();

    let clamp_chat = Clamp::builder().maximum_size(750).build();
    
    let chat_box = GtkBox::new(Orientation::Vertical, 12);
    chat_box.set_margin_top(24);
    chat_box.set_margin_bottom(24);
    chat_box.set_margin_start(16);
    chat_box.set_margin_end(16);
    
    clamp_chat.set_child(Some(&chat_box));
    scroll_window.set_child(Some(&clamp_chat));
    
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&scroll_window);

    // --- Input Area ---
    let input_container = GtkBox::new(Orientation::Vertical, 0);
    input_container.add_css_class("input-bar");

    let clamp_input = Clamp::builder().maximum_size(750).build();
    let input_box = GtkBox::new(Orientation::Horizontal, 8);
    
    let prompt_entry = Entry::builder()
        .placeholder_text("Ask Gemini to write code, analyze files, or fix bugs...")
        .hexpand(true)
        .css_classes(["pill-entry"])
        .build();
    
    let send_button = Button::builder()
        .icon_name("send-symbolic")
        .css_classes(["suggested-action", "send-btn"])
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
    // Passing: (Prompt, Working Directory, YOLO Mode)
    let (async_sender, mut async_receiver) = mpsc::channel::<(String, PathBuf, bool)>(32);

    let chat_box_clone = chat_box.clone();
    let scroll_window_clone = scroll_window.clone();
    
    let current_bot_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let current_system_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));

    // --- UI Update Loop (GTK Thread) ---
    glib::spawn_future_local(async move {
        while let Some(msg) = ui_receiver.recv().await {
            match msg {
                UiMessage::NewUserMessage(m) => {
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
                        let label = Label::builder().label(&m).wrap(true).xalign(0.0).selectable(true).build();
                        let bubble = GtkBox::builder().css_classes(["bot-bubble"]).halign(Align::Start).build();
                        bubble.append(&label);
                        chat_box_clone.append(&bubble);
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

                // Inject Permissions logic!
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