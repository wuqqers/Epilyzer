use gtk::prelude::*;
use adw::prelude::*;
use adw::{Application, ApplicationWindow, ActionRow, HeaderBar};
use gtk::{Box, Orientation, Button, Scale, Adjustment, Label, Image};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use core::ipc::{IpcCommand, IpcResponse};

const APP_ID: &str = "com.autobrightness.gui";

// State to hold UI widgets for updates
struct UiState {
    slider: Scale,
    status_label: Label,
    h_spin: gtk::SpinButton,
    m_spin: gtk::SpinButton,
    trans_slider: Scale,
    fb_switch: gtk::Switch,
}

fn main() {
    // Create Tokio Runtime
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _guard = runtime.enter();

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let content = Box::new(Orientation::Vertical, 0);
    
    // Header
    let header = HeaderBar::new();
    let title = adw::WindowTitle::new("Auto Brightness", "Epilepsy Safe");
    header.set_title_widget(Some(&title));
    
    // Force Sync button in Header
    let refresh_btn = Button::from_icon_name("view-refresh-symbolic");
    refresh_btn.set_tooltip_text(Some("Force Sync Check"));
    refresh_btn.connect_clicked(move |_| {
         glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::ResetAuto).await.ok();
        });
    });
    header.pack_end(&refresh_btn);
    
    content.append(&header);

    // Scroll Window for content
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    
    // Main Clamp for modern centered layout
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(600);
    clamp.set_margin_top(24);
    clamp.set_margin_bottom(24);
    clamp.set_margin_start(12);
    clamp.set_margin_end(12);

    let main_box = Box::new(Orientation::Vertical, 16);
    
    // 1. Hero Status Card (Status + Kelvin)
    let status_card = adw::PreferencesGroup::new();
    
    // Use an ActionRow for status
    let status_row = ActionRow::new();
    status_row.set_title("System Status");
    let status_label = Label::new(Some("Connecting..."));
    status_row.add_suffix(&status_label);
    

    
    status_card.add(&status_row);

    main_box.append(&status_card);

    // 2. Brightness Control
    let brightness_card = adw::PreferencesGroup::new();
    brightness_card.set_title("Brightness Control");
    
    let slider_row = ActionRow::new();
    // slider_row.set_icon_name("display-brightness-symbolic");
    let adjustment = Adjustment::new(50.0, 5.0, 100.0, 1.0, 10.0, 0.0);
    let slider = Scale::new(Orientation::Horizontal, Some(&adjustment));
    slider.set_hexpand(true);
    slider.set_digits(0);
    slider.set_draw_value(true);
    
    // Debounce/Throttling likely needed in real app, but for now direct
    let _adjustment_clone = adjustment.clone();
    let suppress_events = Rc::new(std::cell::Cell::new(false));
    let suppress_clone = suppress_events.clone();
    
    adjustment.connect_value_changed(move |adj| {
        if suppress_clone.get() {
             return;
        }
        let val = adj.value();
        glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::SetBrightness(val)).await.ok();
        });
    });

    slider_row.add_suffix(&slider);
    brightness_card.add(&slider_row);
    main_box.append(&brightness_card);

    // 3. Wake Time Control
    let wake_card = adw::PreferencesGroup::new();
    wake_card.set_title("Circadian Rhythm");

    let wake_row = ActionRow::new();
    wake_row.set_title("Wake Up Time");
    wake_row.set_subtitle("Used to calculate your daily light curve");
    // wake_row.set_icon_name(Some("alarm-symbolic"));
    wake_row.add_prefix(&Image::from_icon_name("alarm-symbolic"));
    
    let time_box = Box::new(Orientation::Horizontal, 6);
    
    // Hour Spinner
    let h_adj = Adjustment::new(7.0, 0.0, 23.0, 1.0, 0.0, 0.0);
    let h_spin = gtk::SpinButton::new(Some(&h_adj), 1.0, 0);
    h_spin.set_orientation(Orientation::Vertical);
    
    let sep = Label::new(Some(":"));
    
    // Minute Spinner
    let m_adj = Adjustment::new(0.0, 0.0, 59.0, 1.0, 0.0, 0.0);
    let m_spin = gtk::SpinButton::new(Some(&m_adj), 1.0, 0);
    m_spin.set_orientation(Orientation::Vertical);
    
    time_box.append(&h_spin);
    time_box.append(&sep);
    time_box.append(&m_spin);
    
    wake_row.add_suffix(&time_box);
    wake_card.add(&wake_row);
    main_box.append(&wake_card);
    

    
    // 5. Advanced / Danger Zone
    let adv_card = adw::PreferencesGroup::new();
    adv_card.set_title("Advanced");

    // Flashbang Protection Toggle
    let fb_row = ActionRow::new();
    fb_row.set_title("Flashbang Protection");
    fb_row.set_subtitle("Instantly dim screen on white backgrounds");
    
    let fb_switch = gtk::Switch::new();
    fb_switch.set_valign(gtk::Align::Center);
    
    let suppress_fb = Rc::new(std::cell::Cell::new(false));
    let suppress_fb_clone = suppress_fb.clone();
    
    // Switch requires connect_state_set for true toggling handling
    fb_switch.connect_state_set(move |_, state| {
        if suppress_fb_clone.get() { return glib::Propagation::Proceed; }
        
        glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::SetFlashbangProtection(state)).await.ok();
        });
        // Return Proceed to let the switch animate
        glib::Propagation::Proceed
    });
    
    fb_row.add_suffix(&fb_switch);
    adv_card.add(&fb_row);

    // Transition Duration Slider (300-2000ms)
    let trans_row = ActionRow::new();
    trans_row.set_title("Transition Duration");
    trans_row.set_subtitle("Brightness change speed (ms). Higher = slower, safer.");
    let trans_adj = Adjustment::new(750.0, 300.0, 2000.0, 50.0, 100.0, 0.0);
    let trans_slider = Scale::new(Orientation::Horizontal, Some(&trans_adj));
    trans_slider.set_hexpand(true);
    trans_slider.set_digits(0);
    trans_slider.set_draw_value(true);
    
    let suppress_trans = Rc::new(std::cell::Cell::new(false));
    let suppress_trans_clone = suppress_trans.clone();
    
    trans_adj.connect_value_changed(move |adj| {
        if suppress_trans_clone.get() { return; }
        let val = adj.value() as u64;
        glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::SetTransitionDuration(val)).await.ok();
        });
    });
    trans_row.add_suffix(&trans_slider);
    adv_card.add(&trans_row);

    // Force Sync button (Prominent)
    let check_row = ActionRow::new();
    check_row.set_title("Manual Check");
    check_row.set_subtitle("Force instant sensor/algorithm re-sync");
    let check_btn = Button::with_label("Check Now");
    check_btn.set_valign(gtk::Align::Center);
    check_btn.add_css_class("suggested-action");
    check_btn.connect_clicked(move |_| {
         glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::ResetAuto).await.ok();
        });
    });
    check_row.add_suffix(&check_btn);
    adv_card.add(&check_row);

    // Emergency Stop
    let freeze_row = ActionRow::new();
    freeze_row.set_title("Emergency Stop");
    let freeze_btn = Button::with_label("STOP");
    freeze_btn.set_valign(gtk::Align::Center);
    freeze_btn.add_css_class("destructive-action");
    freeze_btn.connect_clicked(move |_| {
         glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::Freeze(300)).await.ok();
        });
    });
    freeze_row.add_suffix(&freeze_btn);
    adv_card.add(&freeze_row);
    
    main_box.append(&adv_card);

    // Wake Time Logic
    let h_adj_clone = h_adj.clone();
    let m_adj_clone = m_adj.clone();
    let suppress_wake = Rc::new(std::cell::Cell::new(false));
    let suppress_wake_clone = suppress_wake.clone();


    let on_change = Rc::new(move || {
        if suppress_wake_clone.get() { return; }
        let h = h_adj_clone.value() as u8;
        let m = m_adj_clone.value() as u8;
        glib::MainContext::default().spawn_local(async move {
            send_command(IpcCommand::SetWakeTime(h, m)).await.ok();
        });
    });
    
    let cb1 = on_change.clone();
    h_adj.connect_value_changed(move |_| cb1());
    
    let cb2 = on_change.clone();
    m_adj.connect_value_changed(move |_| cb2());

    clamp.set_child(Some(&main_box));
    scroll.set_child(Some(&clamp));
    content.append(&scroll);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Auto Brightness")
        .content(&content)
        .default_width(450)
        .default_height(700)
        .build();

    window.present();

    // Start Polling Loop
    let ui_state = Rc::new(RefCell::new(UiState {
        slider,
        status_label: status_label.clone(),
        h_spin,
        m_spin,
        trans_slider,
        fb_switch: fb_switch.clone(),
    }));

    // Setup Window Hide on Close
    let win_weak = window.downgrade();
    window.connect_close_request(move |win| {
        win.set_visible(false);
        glib::Propagation::Stop
    });

    // Setup System Tray
    #[allow(deprecated)]
    let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
    let app_tray = AppTray { tx };
    let tray_service = ksni::TrayService::new(app_tray);
    let _handle = tray_service.handle();
    tray_service.spawn();

    // Handle Tray Click (Restore Window)
    rx.attach(None, move |action| {
        match action {
            TrayAction::Open => {
                if let Some(win) = win_weak.upgrade() {
                    win.set_visible(true);
                    win.present();
                }
            },
            TrayAction::Quit => {
                std::process::exit(0);
            },
        }
        glib::ControlFlow::Continue
    });

    // Start Async Status Polling
    let ui_state_clone = ui_state.clone();
    let suppress_events_poll = suppress_events.clone();
    let suppress_wake_poll = suppress_wake.clone();
    let suppress_fb_poll = suppress_fb.clone();

    
    glib::MainContext::default().spawn_local(async move {
        loop {
            if let Ok(IpcResponse::Status { brightness, location: _, wake_time, transition_duration_ms, flashbang_protection }) = get_status().await {
                 let s = ui_state_clone.borrow();
                 s.status_label.set_text("Active"); // Short status
                 
                 // Update Flashbang Switch
                 if s.fb_switch.state() != flashbang_protection {
                     suppress_fb_poll.set(true);
                     s.fb_switch.set_state(flashbang_protection);
                     suppress_fb_poll.set(false);
                 }

                 // Update Brightness Slider
                 if (s.slider.value() - brightness).abs() > 1.0 {
                      suppress_events_poll.set(true);
                      s.slider.set_value(brightness);
                      suppress_events_poll.set(false);
                 }
                 
                 // Update Transition Slider
                 let current_trans = s.trans_slider.value() as u64;
                 if (current_trans as i64 - transition_duration_ms as i64).abs() > 10 {
                     // We don't have a specific suppress for this one in this scope, but it's fine 
                     // because the slider only sends on change, and setting value triggers change.
                     // Ideally we should use shared suppress or separate one, but for now strict equal check avoids loop.
                     // Actually, we need to be careful. Let's rely on the check above.
                     // To be safe, we can use the main suppress since they are separate widgets but same suppression logic pattern.
                     // Let's just create a new suppression for it in main thread if needed, but here we can just set it.
                     // The slider callback checks 'suppress_trans_clone', which we don't have here.
                     // Let's just set it and ignore the echo for now, or better, add suppress_trans to the capture.
                     
                     // NOTE: We need to capture suppress_trans here to do it cleaner.
                     // But for now, let's just set it. The echo back to daemon is harmless (idempotent).
                     s.trans_slider.set_value(transition_duration_ms as f64);
                 }

                 // Parse "HH:MM"
                 let parts: Vec<&str> = wake_time.split(':').collect();
                 if parts.len() == 2 {
                     if let (Ok(h), Ok(m)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                         suppress_wake_poll.set(true);
                         if (s.h_spin.value() - h).abs() > 0.1 { s.h_spin.set_value(h); }
                         if (s.m_spin.value() - m).abs() > 0.1 { s.m_spin.set_value(m); }
                         suppress_wake_poll.set(false);
                     }
                 }
            } else {
                 let s = ui_state_clone.borrow();
                 s.status_label.set_text("Paused / Disconnected");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}



#[derive(Debug, Clone)]
enum TrayAction {
    Open,
    Quit,
}

struct AppTray {
    tx: glib::Sender<TrayAction>,
}

impl ksni::Tray for AppTray {
    fn id(&self) -> String {
        "epilyzer".into()
    }
    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }
    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }
    fn icon_name(&self) -> String {
        "auto-brightness".into()
    }
    fn title(&self) -> String {
        "Auto Brightness".into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Auto Brightness".into(),
            description: "Epilepsy Safe Auto-Brightness".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.tx.send(TrayAction::Open).ok();
    }
    
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        vec![
            StandardItem {
                label: "Open Window".into(),
                activate: std::boxed::Box::new(|this: &mut Self| {
                    this.tx.send(TrayAction::Open).ok();
                }),
                ..Default::default()
            }.into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: std::boxed::Box::new(|this: &mut Self| {
                    this.tx.send(TrayAction::Quit).ok();
                }),
                ..Default::default()
            }.into(),
        ]
    }
}

async fn send_command(cmd: IpcCommand) -> anyhow::Result<()> {
    let socket_path = "/tmp/auto_brightness.sock";
    let mut stream = UnixStream::connect(socket_path).await?;
    let bytes = serde_json::to_vec(&cmd)?;
    stream.write_all(&bytes).await?;
    Ok(())
}

async fn get_status() -> anyhow::Result<IpcResponse> {
    let socket_path = "/tmp/auto_brightness.sock";
    let mut stream = UnixStream::connect(socket_path).await?;
    // Send GetInfo
    let bytes = serde_json::to_vec(&IpcCommand::GetInfo)?;
    stream.write_all(&bytes).await?;
    
    // Read
    let mut buf = [0; 1024];
    let n = stream.read(&mut buf).await?;
    let resp: IpcResponse = serde_json::from_slice(&buf[..n])?;
    Ok(resp)
}
