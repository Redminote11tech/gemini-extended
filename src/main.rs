use libadwaita as adw;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar};
use gtk4::{Align, Box as GtkBox, Button, Entry, Orientation, ScrolledWindow, Label};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use serde::{Deserialize, Serialize};

/// Represents a UI update message sent from the Tokio backend to the GTK main thread.
pub enum UiMessage {
    NewUserMessage(String),
    SystemMessage(String),
    EngineOutput(String),
    Error(String),
}

/// Simple JSON structure to parse the stream-json output
#[derive(Deserialize, Debug)]
struct GeminiStreamOutput {
    text: Option<String>,
    error: Option<String>,
}

fn main() -> glib::ExitCode {
    // Initialize the libadwaita application
    let app = Application::builder()
        .application_id("com.github.redminote11tech.GeminiExtended")
        .build();

    app.connect_activate(move |app| {
        build_ui(app);
    });

    app.run()
}

fn build_ui(app: &Application) {
    // 1. Setup the main window and GTK Box layout
    let window = ApplicationWindow::builder()
        .application(app)
        .title("Gemini Extended")
        .default_width(800)
        .default_height(600)
        .build();

    let main_box = GtkBox::new(Orientation::Vertical, 0);

    // 2. Setup the HeaderBar
    let header_bar = HeaderBar::builder().build();
    main_box.append(&header_bar);

    // 3. Setup the Chat History View (ScrolledWindow containing a VBox)
    let scroll_window = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    let chat_box = GtkBox::new(Orientation::Vertical, 12);
    chat_box.set_margin_top(12);
    chat_box.set_margin_bottom(12);
    chat_box.set_margin_start(12);
    chat_box.set_margin_end(12);
    scroll_window.set_child(Some(&chat_box));
    main_box.append(&scroll_window);

    // 4. Setup the Input Area
    let input_box = GtkBox::new(Orientation::Horizontal, 8);
    input_box.set_margin_start(12);
    input_box.set_margin_end(12);
    input_box.set_margin_top(12);
    input_box.set_margin_bottom(12);

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
    main_box.append(&input_box);

    window.set_content(Some(&main_box));

    // 5. Establish IPC Channels
    // Channel A: Tokio -> GTK
    let (ui_sender, mut ui_receiver) = mpsc::channel::<UiMessage>(100);
    // Channel B: GTK -> Tokio
    let (async_sender, mut async_receiver) = mpsc::channel::<String>(32);

    // 6. Handle incoming messages to update the GTK UI
    let chat_box_clone = chat_box.clone();
    let scroll_window_clone = scroll_window.clone();
    
    glib::spawn_future_local(async move {
        while let Some(msg) = ui_receiver.recv().await {
            let text = match msg {
                UiMessage::NewUserMessage(m) => format!("User: {}", m),
                UiMessage::SystemMessage(m) => format!("System: {}", m),
                UiMessage::EngineOutput(m) => format!("Gemini: {}", m),
                UiMessage::Error(m) => format!("Error: {}", m),
            };

            let label = Label::builder()
                .label(&text)
                .wrap(true)
                .xalign(0.0)
                .selectable(true)
                .build();

            chat_box_clone.append(&label);

            // Auto-scroll to bottom (Wait for GTK to update layout)
            let adjustment = scroll_window_clone.vadjustment();
            adjustment.set_value(adjustment.upper() - adjustment.page_size());
        }
    });

    // 7. Spawn the Async Backend loop
    let ui_sender_clone = ui_sender.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async move {
            while let Some(prompt) = async_receiver.recv().await {
                ui_sender_clone.send(UiMessage::SystemMessage("Spawning gemini-cli...".to_string())).await.unwrap();

                let mut child = match Command::new("gemini")
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
                        continue;
                    }
                };

                let stdout = child.stdout.take().expect("Failed to grab stdout");
                let mut reader = BufReader::new(stdout).lines();

                while let Ok(Some(line)) = reader.next_line().await {
                    if let Ok(parsed) = serde_json::from_str::<GeminiStreamOutput>(&line) {
                        if let Some(txt) = parsed.text {
                            ui_sender_clone.send(UiMessage::EngineOutput(txt)).await.unwrap();
                        }
                        if let Some(err) = parsed.error {
                            ui_sender_clone.send(UiMessage::Error(err)).await.unwrap();
                        }
                    } else {
                        ui_sender_clone.send(UiMessage::EngineOutput(line)).await.unwrap();
                    }
                }
                
                ui_sender_clone.send(UiMessage::SystemMessage("CLI process finished.".to_string())).await.unwrap();
            }
        });
    });

    // 8. Connect User Input Events
    let async_sender_clone = async_sender.clone();
    let prompt_entry_clone = prompt_entry.clone();
    let ui_sender_input = ui_sender.clone();

    let send_action = move || {
        let text = prompt_entry_clone.text().to_string();
        if !text.trim().is_empty() {
            prompt_entry_clone.set_text("");
            
            let async_sender_local = async_sender_clone.clone();
            let ui_sender_local = ui_sender_input.clone();
            
            glib::spawn_future_local(async move {
                ui_sender_local.send(UiMessage::NewUserMessage(text.clone())).await.unwrap();
                if let Err(e) = async_sender_local.send(text).await {
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