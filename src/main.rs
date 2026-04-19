use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar, ToolbarView, Clamp};
use gtk4::{Align, Box as GtkBox, Button, Entry, Orientation, ScrolledWindow, Label, CssProvider, FileDialog};
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
        ".user-bubble { 
            background-color: @accent_bg_color; 
            color: @accent_fg_color; 
            border-radius: 12px; 
            padding: 12px; 
            margin-bottom: 8px; 
        }
        .bot-bubble { 
            background-color: @card_bg_color; 
            color: @card_fg_color; 
            border-radius: 12px; 
            padding: 12px; 
            margin-bottom: 8px; 
            border: 1px solid @card_shade_color;
        }
        .system-bubble { 
            color: @dim_label_color; 
            font-style: italic; 
            font-size: smaller;
            margin-bottom: 4px;
        }"
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
        .default_height(700)
        .build();

    let toolbar_view = ToolbarView::builder().build();
    let header_bar = HeaderBar::builder().build();
    toolbar_view.add_top_bar(&header_bar);

    let current_dir = Rc::new(RefCell::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))));

    let dir_button = Button::builder()
        .icon_name("folder-open-symbolic")
        .tooltip_text("Set Working Directory")
        .build();
    let current_dir_label = Label::builder()
        .label(&*current_dir.borrow().to_string_lossy())
        .css_classes(["dim-label"])
        .margin_start(8)
        .build();

    let header_box = GtkBox::new(Orientation::Horizontal, 4);
    header_box.append(&dir_button);
    header_box.append(&current_dir_label);
    header_bar.pack_start(&header_box);

    let window_clone = window.clone();
    let current_dir_clone = current_dir.clone();
    let current_dir_label_clone = current_dir_label.clone();
    dir_button.connect_clicked(move |_| {
        let dialog = FileDialog::builder().title("Select Working Directory").build();
        let current_dir_clone = current_dir_clone.clone();
        let current_dir_label_clone = current_dir_label_clone.clone();
        dialog.select_folder(Some(&window_clone), gtk4::gio::Cancellable::NONE, move |result| {
            if let Ok(folder) = result {
                if let Some(path) = folder.path() {
                    *current_dir_clone.borrow_mut() = path.clone();
                    current_dir_label_clone.set_label(&*path.to_string_lossy());
                }
            }
        });
    });

    let scroll_window = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();

    let clamp_chat = Clamp::builder().maximum_size(800).build();
    
    let chat_box = GtkBox::new(Orientation::Vertical, 12);
    chat_box.set_margin_top(24);
    chat_box.set_margin_bottom(24);
    chat_box.set_margin_start(24);
    chat_box.set_margin_end(24);
    
    clamp_chat.set_child(Some(&chat_box));
    scroll_window.set_child(Some(&clamp_chat));
    
    let main_box = GtkBox::new(Orientation::Vertical, 0);
    main_box.append(&scroll_window);

    let clamp_input = Clamp::builder().maximum_size(800).build();
    let input_box = GtkBox::new(Orientation::Horizontal, 8);
    input_box.set_margin_start(24);
    input_box.set_margin_end(24);
    input_box.set_margin_top(12);
    input_box.set_margin_bottom(24);
    input_box.add_css_class("linked");

    let prompt_entry = Entry::builder()
        .placeholder_text("Ask Gemini to write code, execute a command, or analyze files...")
        .hexpand(true)
        .build();
    
    let send_button = Button::builder()
        .label("Send")
        .css_classes(["suggested-action"])
        .build();

    input_box.append(&prompt_entry);
    input_box.append(&send_button);
    clamp_input.set_child(Some(&input_box));
    main_box.append(&clamp_input);

    toolbar_view.set_content(Some(&main_box));
    window.set_content(Some(&toolbar_view));

    let (ui_sender, mut ui_receiver) = mpsc::channel::<UiMessage>(100);
    let (async_sender, mut async_receiver) = mpsc::channel::<(String, PathBuf)>(32);

    let chat_box_clone = chat_box.clone();
    let scroll_window_clone = scroll_window.clone();
    
    let current_bot_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));
    let current_system_label: Rc<RefCell<Option<Label>>> = Rc::new(RefCell::new(None));

    glib::spawn_future_local(async move {
        while let Some(msg) = ui_receiver.recv().await {
            match msg {
                UiMessage::NewUserMessage(m) => {
                    *current_bot_label.borrow_mut() = None; // Reset bot label
                    *current_system_label.borrow_mut() = None; // Reset system label

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

    let ui_sender_clone = ui_sender.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            while let Some((prompt, cwd)) = async_receiver.recv().await {
                ui_sender_clone.send(UiMessage::SystemMessage("Thinking...".to_string())).await.unwrap();

                let mut child = match Command::new("gemini")
                    .current_dir(&cwd)
                    .arg("-p")
                    .arg(&prompt)
                    .arg("-o")
                    .arg("stream-json")
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn() 
                {
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

    let async_sender_clone = async_sender.clone();
    let prompt_entry_clone = prompt_entry.clone();
    let ui_sender_input = ui_sender.clone();
    let current_dir_action = current_dir.clone();

    let send_action = move || {
        let text = prompt_entry_clone.text().to_string();
        if !text.trim().is_empty() {
            prompt_entry_clone.set_text("");
            
            let async_sender_local = async_sender_clone.clone();
            let ui_sender_local = ui_sender_input.clone();
            let cwd = current_dir_action.borrow().clone();
            
            glib::spawn_future_local(async move {
                ui_sender_local.send(UiMessage::NewUserMessage(text.clone())).await.unwrap();
                if let Err(e) = async_sender_local.send((text, cwd)).await {
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