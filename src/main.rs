use adw::prelude::*;
use adw::{Application, ApplicationWindow, HeaderBar};
use glib::clone;
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

/// Simple JSON structure to parse the stream-json output (can be expanded later)
#[derive(Deserialize, Debug)]
struct GeminiStreamOutput {
    // Modify based on the actual CLI stream-json schema
    text: Option<String>,
    error: Option<String>,
}

fn main() -> glib::ExitCode {
    // Initialize the Tokio runtime
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create Tokio runtime");

    // Initialize the libadwaita application
    let app = Application::builder()
        .application_id("com.github.redminote11tech.GeminiExtended")
        .build();

    app.connect_activate(move |app| {
        build_ui(app, &rt);
    });

    app.run()
}

fn build_ui(app: &Application, rt: &tokio::runtime::Runtime) {
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
    // Channel A: Tokio -> GTK (glib::MainContext::channel)
    let (ui_sender, ui_receiver) = glib::MainContext::channel::<UiMessage>(glib::Priority::DEFAULT);
    
    // Channel B: GTK -> Tokio (mpsc)
    let (async_sender, mut async_receiver) = mpsc::channel::<String>(32);

    // 6. Handle incoming messages to update the GTK UI
    ui_receiver.attach(
        None,
        clone!(@weak chat_box, @weak scroll_window => @default-return glib::ControlFlow::Break,
            move |msg| {
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

                chat_box.append(&label);

                // Auto-scroll to bottom
                let adjustment = scroll_window.vadjustment();
                adjustment.set_value(adjustment.upper() - adjustment.page_size());

                glib::ControlFlow::Continue
            }
        ),
    );

    // 7. Spawn the Async Backend loop
    let ui_sender_clone = ui_sender.clone();
    rt.spawn(async move {
        while let Some(prompt) = async_receiver.recv().await {
            ui_sender_clone.send(UiMessage::SystemMessage("Spawning gemini-cli...".to_string())).unwrap();

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
                    ui_sender_clone.send(UiMessage::Error(format!("Failed to spawn CLI: {}", e))).unwrap();
                    continue;
                }
            };

            let stdout = child.stdout.take().expect("Failed to grab stdout");
            let mut reader = BufReader::new(stdout).lines();

            while let Ok(Some(line)) = reader.next_line().await {
                // Parse JSON stream from CLI
                if let Ok(parsed) = serde_json::from_str::<GeminiStreamOutput>(&line) {
                    if let Some(txt) = parsed.text {
                        ui_sender_clone.send(UiMessage::EngineOutput(txt)).unwrap();
                    }
                    if let Some(err) = parsed.error {
                        ui_sender_clone.send(UiMessage::Error(err)).unwrap();
                    }
                } else {
                    // Fallback for raw text just in case
                    ui_sender_clone.send(UiMessage::EngineOutput(line)).unwrap();
                }
            }

            ui_sender_clone.send(UiMessage::SystemMessage("CLI process finished.".to_string())).unwrap();
        }
    });

    // 8. Connect User Input Events
    let send_action = clone!(@weak prompt_entry, @strong async_sender, @strong ui_sender => move || {
        let text = prompt_entry.text().to_string();
        if !text.trim().is_empty() {
            prompt_entry.set_text("");
            ui_sender.send(UiMessage::NewUserMessage(text.clone())).unwrap();
            
            let async_sender = async_sender.clone();
            glib::spawn_future_local(async move {
                if let Err(e) = async_sender.send(text).await {
                    eprintln!("Failed to send to async runtime: {}", e);
                }
            });
        }
    });

    send_button.connect_clicked(clone!(@strong send_action => move |_| send_action()));
    prompt_entry.connect_activate(move |_| send_action());

    window.present();
}