#![cfg_attr(feature = "ui-only", allow(dead_code))]
#![cfg_attr(feature = "ui-only", allow(unused_imports))]
#![cfg_attr(feature = "ui-only", allow(unused_variables))]
#![cfg_attr(test, recursion_limit = "128")]

use clap::{App, Arg};
use client::bandwidth_client::BandwidthClient;
use custom_error::custom_error;
use gio::prelude::*;
use gtk::prelude::*;
use log::*;
use provider_drone::DEFAULT_DRONE_PORT;
use pubsub_client::client::{start_pubsub, Event};
use pubsub_client::request::PubSubRequest;
use serde_derive::Deserialize;
use serde_json::Value;
use solana_client::rpc_client::RpcClient;
use solana_sdk::account::Account;
use solana_sdk::pubkey::read_pubkey;
use solana_sdk::signature::{read_keypair, KeypairUtil};
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::process::{Child, Command};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, RecvError, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use stream_video::stream_video::*;

const NUM_DESTINATIONS: usize = 3;

const CALL_BUTTON_SIZE: i32 = 175;
const END_CALL_BUTTON_SIZE: i32 = 50;
const END_CALL_BUTTON_X_LOCATION: i32 = 730;
const END_CALL_BUTTON_Y_LOCATION: i32 = 20;

const STYLE: &str = "
window {
    background-color: black;
}
button {
    border-style: none;
    outline-style: none;
    box-shadow: none;
    color: #000000;
    background-image: none;
    text-shadow: none;
    -gtk-icon-shadow: none;
    -gtk-icon-effect: none;
}
button:hover {
    -gtk-icon-shadow: none;
    -gtk-icon-effect: none;
}
button:active {
    -gtk-icon-effect: dim;
}
button.disabled {
    -gtk-icon-effect: dim;
}
#token-button {
    border-radius: 12px;
    font-weight: bold;
    font-size: 24px;
    background-color: #00ffbb;
}
#token-button:active {
    background-color: #00cc96;
}
#end-call-button {
    font-weight: bold;
    font-size: 24px;
    color: #ffffff;
    background-color: #ff0000;
}
#end-call-button:active {
    background-color: #cc0000;
}
#token-status {
    font-size: 32px;
    color: #00ffbb;
}";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Data Counter Tester")
        .arg(
            Arg::with_name("airdrop-lamports")
                .short("a")
                .long("airdrop-lamports")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of lamports to request in an airdrop"),
        )
        .arg(
            Arg::with_name("contract-lamports")
                .short("l")
                .long("contract-lamports")
                .value_name("NUM")
                .takes_value(true)
                .help("Number of lamports to fund contract with"),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("/path/to/config/file")
                .takes_value(true)
                .help("/path/to/config.toml, defaults to 'config-local/config.toml'"),
        )
        .get_matches();

    let config = matches
        .value_of("config")
        .unwrap_or("config-local/config.toml");
    let mut config = File::open(config)?;

    let mut config_str = Vec::new();
    config.read_to_end(&mut config_str)?;
    let config_str = String::from_utf8(config_str)?;

    let config: Config = toml::from_str(&config_str)?;

    info!("Sucessfully read config");

    let client_account = read_keypair(&config.keypair)?;
    let gatekeeper_pubkey = read_pubkey(&config.gatekeeper_pubkey)?;
    let provider_pubkey = read_pubkey(&config.provider_pubkey)?;

    let client_pubkey = client_account.pubkey();

    // ToSocketAddrs requires a port, so add the rpc_port for lookup
    debug!("Looking up fullnode hostname");
    let fullnode_addrs: Vec<SocketAddr> = (config.fullnode.as_str(), config.rpc_port)
        .to_socket_addrs()?
        .collect();
    info!(
        "Fullnode address: {}, found hosts: {:?}",
        config.fullnode, fullnode_addrs
    );

    if fullnode_addrs.is_empty() {
        error!("Failed to lookup fullnode hostname");
        Err(GuiError::HostnameLookupFailed {
            hostname: config.fullnode.clone(),
        })?;
    }

    // Copy the IP address found for rpc to the pubsub address
    let rpc_addr = fullnode_addrs[0];
    let mut pubsub_addr = rpc_addr;
    pubsub_addr.set_port(config.pubsub_port);

    // Set up Solana bandwidth prepayment contract
    let contract_lamports: u64 = if let Some(lamport_str) = matches.value_of("contract-lamports") {
        lamport_str.parse().unwrap()
    } else {
        config.default_contract_lamports
    };

    let airdrop_lamports: u64 = if let Some(lamport_str) = matches.value_of("airdrop-lamports") {
        lamport_str.parse().unwrap()
    } else {
        config.default_airdrop_lamports
    };

    debug!("Looking up gatekeeper hostname");
    let gatekeeper_addrs: Vec<SocketAddr> =
        (config.gatekeeper_addr.as_str(), config.gatekeeper_port)
            .to_socket_addrs()?
            .collect();
    info!(
        "Gatekeeper address: {}, found hosts: {:?}",
        config.gatekeeper_addr, gatekeeper_addrs
    );

    if gatekeeper_addrs.is_empty() {
        error!("Failed to lookup fullnode hostname");
        Err(GuiError::HostnameLookupFailed {
            hostname: config.gatekeeper_addr.clone(),
        })?;
    }

    let gatekeeper_addr = gatekeeper_addrs[0];

    let destinations: Vec<Option<SocketAddr>> = config
        .destinations
        .as_ref()
        .iter()
        .map(|dest| {
            let dest = dest.as_str();
            debug!("Looking up destination hostname: {}", dest);
            let addrs: Vec<SocketAddr> = match dest.to_socket_addrs() {
                Ok(iter) => iter.collect(),
                Err(e) => {
                    warn!("Failed to lookup hostname \"{}\". Error: \"{}\"", dest, e);
                    return None;
                }
            };
            debug!("Found hosts: {:?}", addrs);
            if addrs.is_empty() {
                None
            } else {
                Some(addrs[0])
            }
        })
        .collect();

    info!("Destinations: {:?}", destinations);

    #[cfg(not(feature = "ui-only"))]
    let fullnode_client = RpcClient::new_socket(rpc_addr);
    #[cfg(not(feature = "ui-only"))]
    let client = Arc::new(BandwidthClient::new(client_account, fullnode_client));
    #[cfg(not(feature = "ui-only"))]
    let client_clone = client.clone();

    #[cfg(not(feature = "ui-only"))]
    let fullnode_client = RpcClient::new_socket(rpc_addr);

    #[cfg(not(feature = "ui-only"))]
    let mut drone_addr = rpc_addr;
    #[cfg(not(feature = "ui-only"))]
    drone_addr.set_port(DEFAULT_DRONE_PORT);

    #[cfg(not(feature = "ui-only"))]
    let (listener_send, listener_recv) = channel();

    #[cfg(not(feature = "ui-only"))]
    let _listener_thread = thread::spawn(move || {
        let mut listen_port: Option<u16> = None;
        let mut status_sender = None;
        'outer: loop {
            let mut listener;
            debug!("Entering listener stopped mode");
            'stopped: loop {
                if let Some(port) = listen_port {
                    info!("Restarting listener on port {}", port);
                    listener =
                        VideoManager::new_video_listener(port, status_sender.as_ref().cloned())
                            .unwrap();
                    break 'stopped;
                } else {
                    match listener_recv.recv() {
                        Ok(ListenerCommand::StartListening(port)) => {
                            info!("Starting listener on port {}", port);
                            listener = VideoManager::new_video_listener(
                                port,
                                status_sender.as_ref().cloned(),
                            )
                            .unwrap();
                            listen_port = Some(port);
                            break 'stopped;
                        }
                        Ok(ListenerCommand::AddStatusSender(sender)) => {
                            status_sender = Some(sender)
                        }
                        Ok(ListenerCommand::StopListening) => {}
                        Ok(ListenerCommand::StopVideo) => {}
                        Err(_) => break 'outer,
                    }
                }
            }

            debug!("Entering listener waiting mode");
            'waiting: loop {
                if listener.video_running.load(Ordering::Acquire) {
                    break 'waiting;
                }
                match listener_recv.try_recv() {
                    Ok(ListenerCommand::StopListening) => {
                        info!("Stopping listener");
                        listener.kill().unwrap();
                        listen_port = None;
                        break 'waiting;
                    }
                    Ok(ListenerCommand::StopVideo) => {
                        info!("Stopping video");
                        listener.kill().unwrap();
                        continue 'outer;
                    }
                    Ok(ListenerCommand::AddStatusSender(sender)) => {
                        status_sender = Some(sender.clone());
                        listener.add_status_sender(sender);
                    }
                    Ok(ListenerCommand::StartListening(_)) => {}
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'outer,
                }
                thread::sleep(Duration::from_millis(5));
            }

            debug!("Entering listener running mode");
            'running: while listener.check_video_running().unwrap() {
                match listener_recv.try_recv() {
                    Ok(ListenerCommand::StopVideo) => {
                        info!("Stopping video");
                        listener.kill().unwrap();
                        break 'running;
                    }
                    Ok(ListenerCommand::AddStatusSender(sender)) => {
                        status_sender = Some(sender.clone());
                        listener.add_status_sender(sender);
                    }
                    Ok(ListenerCommand::StartListening(_)) => {}
                    Ok(ListenerCommand::StopListening) => {}
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'outer,
                }
                thread::sleep(Duration::from_millis(5));
            }
            debug!("Video has stopped");
        }
    });

    #[cfg(not(feature = "ui-only"))]
    listener_send.send(ListenerCommand::StartListening(config.listener_port))?;

    #[cfg(not(feature = "ui-only"))]
    let listener_send_clone = listener_send.clone();
    #[cfg(not(feature = "ui-only"))]
    let listener_port_clone = config.listener_port;

    #[cfg(not(feature = "ui-only"))]
    let (connecter_send, connecter_recv) = channel();

    #[cfg(not(feature = "ui-only"))]
    let _connecter_thread = thread::spawn(move || {
        let mut status_sender = None;
        'outer: loop {
            let mut connecter;
            debug!("Entering connecter stopped mode");
            'stopped: loop {
                match connecter_recv.recv() {
                    Ok(ConnecterCommand::StartConnection(addr, lamports)) => {
                        let prepay_account = client.initialize_contract(
                            lamports,
                            &gatekeeper_pubkey,
                            &provider_pubkey,
                        );

                        info!("Requesting connection to {:?}", addr);
                        let connection_addr = client
                            .request_connection(&gatekeeper_addr, addr, &prepay_account.pubkey())
                            .unwrap();

                        info!("Connecting to {:?}", connection_addr);
                        connecter = VideoManager::new_video_connecter(
                            &connection_addr,
                            status_sender.as_ref().cloned(),
                        )
                        .unwrap();

                        break 'stopped;
                    }
                    Ok(ConnecterCommand::AddStatusSender(sender)) => status_sender = Some(sender),
                    Ok(ConnecterCommand::StopVideo) => {}
                    Err(_) => break 'outer,
                }
            }

            debug!("Entering connecter waiting mode");
            'waiting: loop {
                if connecter.video_running.load(Ordering::Acquire) {
                    break 'waiting;
                }
                match connecter_recv.try_recv() {
                    Ok(ConnecterCommand::StopVideo) => {
                        info!("Stopping video");
                        connecter.kill().unwrap();
                        continue 'outer;
                    }
                    Ok(ConnecterCommand::AddStatusSender(sender)) => {
                        status_sender = Some(sender.clone());
                        connecter.add_status_sender(sender);
                    }
                    Ok(ConnecterCommand::StartConnection(_, _)) => {}
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'outer,
                }
                thread::sleep(Duration::from_millis(5));
            }

            debug!("Entering connecter running mode");
            'running: while connecter.check_video_running().unwrap() {
                match connecter_recv.try_recv() {
                    Ok(ConnecterCommand::StopVideo) => {
                        info!("Stopping video");
                        connecter.kill().unwrap();
                        break 'running;
                    }
                    Ok(ConnecterCommand::AddStatusSender(sender)) => {
                        status_sender = Some(sender.clone());
                        connecter.add_status_sender(sender);
                    }
                    Ok(ConnecterCommand::StartConnection(_, _)) => {}
                    Err(TryRecvError::Empty) => {}
                    Err(TryRecvError::Disconnected) => break 'outer,
                }
                thread::sleep(Duration::from_millis(5));
            }
            debug!("Video has stopped, restarting listener");
            listener_send_clone
                .send(ListenerCommand::StartListening(listener_port_clone))
                .unwrap();
        }
    });

    #[cfg(not(feature = "ui-only"))]
    let exit_button_image_clone = config.exit_button_image.clone();

    #[cfg(not(feature = "ui-only"))]
    let (exit_btn_overlay_send, exit_btn_overlay_recv) = channel();

    #[cfg(not(feature = "ui-only"))]
    let _exit_btn_overlay_thread = thread::spawn(move || {
        let mut child: Option<Child> = None;
        loop {
            match exit_btn_overlay_recv.recv() {
                Ok(ExitButtonOverlayCommand::Enable) => {
                    if child.is_none() {
                        child = Some(
                            Command::new("/home/pi/raspidmx/pngview/pngview")
                                .env("LD_LIBRARY_PATH", "/home/pi/raspidmx/lib/")
                                .args(&["-b", "0", "-l", "3", &exit_button_image_clone])
                                .spawn()
                                .unwrap(),
                        );
                    }
                }
                Ok(ExitButtonOverlayCommand::Disable) => {
                    if let Some(mut child_process) = child {
                        child_process.kill().unwrap();
                        child = None;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let uiapp = gtk::Application::new("org.solana.VideoDemo", gio::ApplicationFlags::FLAGS_NONE)
        .expect("Application::new failed");

    uiapp.connect_activate(move |app| {
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(STYLE.as_bytes())
            .expect("Failed to load CSS");
        // We give the CssProvider to the default screen so the CSS rules we added
        // can be applied to our window.
        gtk::StyleContext::add_provider_for_screen(
            &gdk::Screen::get_default().expect("Error initializing gtk css provider."),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // We create the main window.
        let win = gtk::ApplicationWindow::new(app);

        maybe_fullscreen(&win);

        //win.set_resizable(false);

        // For some reason this breaks the GUI on a mac
        #[cfg(all(target_arch = "arm", target_os = "linux", target_env = "gnu"))]
        win.set_keep_below(true);

        // Then we set its size and a title.
        win.set_default_size(800, 480);
        win.set_title("Video Streamer");

        let call_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);

        call_buttons.set_size_request(650, CALL_BUTTON_SIZE - 25);

        let but0 = gtk::Button::new();
        let but1 = gtk::Button::new();
        let but2 = gtk::Button::new();

        let pix0 = gdk_pixbuf::Pixbuf::new_from_file(&config.images[0]).unwrap();
        let pix1 = gdk_pixbuf::Pixbuf::new_from_file(&config.images[1]).unwrap();
        let pix2 = gdk_pixbuf::Pixbuf::new_from_file(&config.images[2]).unwrap();

        let sc_pix0 = pix0.scale_simple(CALL_BUTTON_SIZE, CALL_BUTTON_SIZE, gdk_pixbuf::InterpType::Bilinear);
        let sc_pix1 = pix1.scale_simple(CALL_BUTTON_SIZE, CALL_BUTTON_SIZE, gdk_pixbuf::InterpType::Bilinear);
        let sc_pix2 = pix2.scale_simple(CALL_BUTTON_SIZE, CALL_BUTTON_SIZE, gdk_pixbuf::InterpType::Bilinear);

        let img0 = gtk::Image::new_from_pixbuf(&sc_pix0);
        let img1 = gtk::Image::new_from_pixbuf(&sc_pix1);
        let img2 = gtk::Image::new_from_pixbuf(&sc_pix2);

        but0.set_image(Some(&img0));
        but1.set_image(Some(&img1));
        but2.set_image(Some(&img2));

        but0.set_always_show_image(true);
        but1.set_always_show_image(true);
        but2.set_always_show_image(true);

        but0.get_style_context().add_class("circular");
        but1.get_style_context().add_class("circular");
        but2.get_style_context().add_class("circular");

        but0.set_size_request(CALL_BUTTON_SIZE - 25, CALL_BUTTON_SIZE - 25);
        but1.set_size_request(CALL_BUTTON_SIZE - 25, CALL_BUTTON_SIZE - 25);
        but2.set_size_request(CALL_BUTTON_SIZE - 25, CALL_BUTTON_SIZE - 25);

        #[cfg(not(feature="ui-only"))]
        {
            if let Some(destination) = destinations[0] {
                let listener_send_clone = listener_send.clone();
                let connecter_send_clone = connecter_send.clone();
                but0.connect_clicked(move |_| {
                    listener_send_clone.send(ListenerCommand::StopListening).unwrap();
                    connecter_send_clone
                        .send(ConnecterCommand::StartConnection(
                            destination,
                            contract_lamports,
                        ))
                        .unwrap();
                });
            } else {
                but0.get_style_context().add_class("disabled");
            }

            if let Some(destination) = destinations[1] {
                let listener_send_clone = listener_send.clone();
                let connecter_send_clone = connecter_send.clone();
                but1.connect_clicked(move |_| {
                    listener_send_clone.send(ListenerCommand::StopListening).unwrap();
                    connecter_send_clone
                        .send(ConnecterCommand::StartConnection(
                            destination,
                            contract_lamports,
                        ))
                        .unwrap();
                });
            } else {
                but1.get_style_context().add_class("disabled");
            }

            if let Some(destination) = destinations[2] {
                let listener_send_clone = listener_send.clone();
                let connecter_send_clone = connecter_send.clone();
                but2.connect_clicked(move |_| {
                    listener_send_clone.send(ListenerCommand::StopListening).unwrap();
                    connecter_send_clone
                        .send(ConnecterCommand::StartConnection(
                            destination,
                            contract_lamports,
                        ))
                        .unwrap();
                });
            } else {
                but2.get_style_context().add_class("disabled");
            }
        }

        call_buttons.add(&but0);
        call_buttons.add(&but1);
        call_buttons.add(&but2);

        call_buttons.set_child_expand(&but0, false);
        call_buttons.set_child_expand(&but1, false);
        call_buttons.set_child_expand(&but2, false);

        call_buttons.set_homogeneous(true);
        call_buttons.set_margin_top(50);
        call_buttons.set_margin_start(75);
        call_buttons.set_margin_end(75);
        call_buttons.set_margin_bottom(25);

        let token_handling = gtk::Box::new(gtk::Orientation::Horizontal, 75);

        #[cfg(not(feature="ui-only"))]
        let balance = if let Ok(Some(bal)) = fullnode_client.retry_get_balance(&client_pubkey, 5)
        {
            bal
        } else {
            0
        };
        #[cfg(feature="ui-only")]
        let balance = "XXXXX";

        #[cfg(not(feature="ui-only"))]
        let currency = if balance == 1 { "token" } else { "tokens" };

        #[cfg(feature="ui-only")]
        let currency = "tokens";

        let token_status = gtk::Label::new(None);
        token_status.set_markup(&format!(
            r#"<span size="medium">You currently have:</span>{}<span size="large">{} {}</span>"#,
            "\n",
            balance,
            currency,
        ));
        token_status.set_justify(gtk::Justification::Center);
        gtk::WidgetExt::set_name(&token_status, "token-status");

        #[cfg(not(feature="ui-only"))]
        let (send, recv) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

        #[cfg(not(feature="ui-only"))]
        thread::spawn(move || {
            let pubsub_thread = start_pubsub(
                format!("ws://{}", pubsub_addr),
                PubSubRequest::Account,
                &client_pubkey,
            )
            .unwrap();
            let pubsub_recv = pubsub_thread.receiver;

            loop {
                if let Some(lamports) = process_pubsub(pubsub_recv.recv()) {
                    send.send(lamports).unwrap();
                }
            }
        });

        let token_button = gtk::Button::new_with_label("Purchase\nTokens");
        gtk::WidgetExt::set_name(&token_button, "token-button");
        let token_button_label = token_button
            .get_child()
            .unwrap()
            .downcast::<gtk::Label>()
            .unwrap();
        token_button_label.set_justify(gtk::Justification::Center);

        #[cfg(not(feature="ui-only"))]
        {
            let client_clone = client_clone.clone();
            token_button.connect_clicked(move |_| {
                client_clone
                    .request_airdrop(&drone_addr, airdrop_lamports)
                    .unwrap();
            });
        }

        token_handling.add(&token_status);
        token_handling.add(&token_button);

        token_handling.set_child_expand(&token_status, true);
        token_handling.set_child_expand(&token_button, true);

        token_handling.set_border_width(50);

        #[cfg(not(feature="ui-only"))]
        recv.attach(None, move |num_tokens| {
            let currency = if num_tokens == 1 { "token" } else { "tokens" };
            token_status.set_markup(&format!(
                r#"<span size="medium">You currently have:</span>{}<span size="large">{} {}</span>"#,
                "\n",
                num_tokens,
                currency,
            ));
            glib::Continue(true)
        });

        let main_layout = gtk::Box::new(gtk::Orientation::Vertical, 0);

        main_layout.add(&call_buttons);
        main_layout.add(&token_handling);

        main_layout.set_child_expand(&call_buttons, true);
        main_layout.set_child_expand(&token_handling, true);

        let end_call_but = gtk::Button::new_with_label("X");
        end_call_but.set_property_expand(false);
        end_call_but.set_property_height_request(END_CALL_BUTTON_SIZE);
        end_call_but.set_property_width_request(END_CALL_BUTTON_SIZE);
        end_call_but.get_style_context().add_class("circular");
        gtk::WidgetExt::set_name(&end_call_but, "end-call-button");

        let call_layout = gtk::Fixed::new();
        call_layout.put(&end_call_but, END_CALL_BUTTON_X_LOCATION, END_CALL_BUTTON_Y_LOCATION);

        let layout_switch = gtk::Stack::new();

        layout_switch.add_named(&main_layout, "main-layout");
        layout_switch.add_named(&call_layout, "call-layout");

        layout_switch.set_visible_child(&main_layout);

        #[cfg(feature="ui-only")]
        {
            if let Some(_) = destinations[0] {
                let call_layout_clone = call_layout.clone();
                let layout_switch_clone = layout_switch.clone();
                but0.connect_clicked(move |_| layout_switch_clone.set_visible_child(&call_layout_clone));
            } else {
                but0.get_style_context().add_class("disabled");
            }

            if let Some(_) = destinations[1] {
                let call_layout_clone = call_layout.clone();
                let layout_switch_clone = layout_switch.clone();
                but1.connect_clicked(move |_| layout_switch_clone.set_visible_child(&call_layout_clone));
            } else {
                but1.get_style_context().add_class("disabled");
            }

            if let Some(_) = destinations[2] {
                let call_layout_clone = call_layout.clone();
                let layout_switch_clone = layout_switch.clone();
                but2.connect_clicked(move |_| layout_switch_clone.set_visible_child(&call_layout_clone));
            } else {
                but2.get_style_context().add_class("disabled");
            }

            let layout_switch_clone = layout_switch.clone();
            end_call_but.connect_clicked(move |_| layout_switch_clone.set_visible_child(&main_layout));
        }

        #[cfg(not(feature="ui-only"))]
        {
            let (send, recv) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

            let send_clone = send.clone();
            listener_send.send(ListenerCommand::AddStatusSender(send_clone)).unwrap();

            let send_clone = send.clone();
            connecter_send.send(ConnecterCommand::AddStatusSender(send_clone)).unwrap();

            let call_layout_clone = call_layout.clone();
            let layout_switch_clone = layout_switch.clone();
            let exit_btn_overlay_send_clone = exit_btn_overlay_send.clone();
            recv.attach(None, move |status| {
                match status {
                    VideoStatus::Starting => {
                        layout_switch_clone.set_visible_child(&call_layout_clone);
                        exit_btn_overlay_send_clone.send(ExitButtonOverlayCommand::Enable).unwrap();
                    }
                    VideoStatus::Stopping => {
                        layout_switch_clone.set_visible_child(&main_layout);
                        exit_btn_overlay_send_clone.send(ExitButtonOverlayCommand::Disable).unwrap();
                    }
                }
                glib::Continue(true)
            });

            let listener_send_clone = listener_send.clone();
            let connecter_send_clone = connecter_send.clone();
            let exit_btn_overlay_send_clone = exit_btn_overlay_send.clone();
            end_call_but.connect_clicked(move |_| {
                listener_send_clone.send(ListenerCommand::StopVideo).unwrap();
                connecter_send_clone.send(ConnecterCommand::StopVideo).unwrap();
                exit_btn_overlay_send_clone.send(ExitButtonOverlayCommand::Disable).unwrap();
            });

        }

        win.add(&layout_switch);

        // Don't forget to make all widgets visible.
        win.show_all();
    });
    uiapp.run(&[]);

    Ok(())
}

#[derive(Deserialize)]
struct Config {
    keypair: String,
    gatekeeper_pubkey: String,
    provider_pubkey: String,
    fullnode: String,
    rpc_port: u16,
    pubsub_port: u16,
    listener_port: u16,
    gatekeeper_addr: String,
    gatekeeper_port: u16,
    default_airdrop_lamports: u64,
    default_contract_lamports: u64,
    destinations: [String; NUM_DESTINATIONS],
    images: [String; NUM_DESTINATIONS],
    exit_button_image: String,
}

enum ListenerCommand {
    StartListening(u16),
    StopListening,
    StopVideo,
    AddStatusSender(glib::Sender<VideoStatus>),
}

enum ConnecterCommand {
    StartConnection(SocketAddr, u64),
    StopVideo,
    AddStatusSender(glib::Sender<VideoStatus>),
}

enum ExitButtonOverlayCommand {
    Enable,
    Disable,
}

custom_error! {GuiError
    HostnameLookupFailed { hostname: String } = "hostname lookup for {hostname} failed.",
}

fn process_pubsub(res: Result<Event, RecvError>) -> Option<u64> {
    match res.unwrap() {
        Event::Message(notification) => {
            let json: Value = serde_json::from_str(&notification.into_text().unwrap()).unwrap();
            let account_json = json["params"]["result"].clone();
            let account: Account = serde_json::from_value(account_json).unwrap();
            info!(
                "received notification. account balance: {}",
                account.lamports
            );
            Some(account.lamports)
        }
        Event::Disconnect(_, _) => {
            panic!("PubSub connection dropped");
        }
        _ => None,
    }
}

// Sets app to fullscreen if running on RPi
#[cfg(all(target_arch = "arm", target_os = "linux", target_env = "gnu"))]
fn maybe_fullscreen(win: &gtk::ApplicationWindow) -> () {
    win.fullscreen()
}

#[cfg(not(all(target_arch = "arm", target_os = "linux", target_env = "gnu")))]
fn maybe_fullscreen(_win: &gtk::ApplicationWindow) {}

#[cfg(test)]
mod tests {
    use crate::process_pubsub;
    use pubsub_client::client::Event;
    use serde_json::json;
    use std::sync::mpsc::channel;

    #[test]
    fn test_pubsub_processor() {
        let json = json!({
            "jsonrpc": "2.0",
            "method":"accountNotification",
            "params": {
                "result": {
                    "data": [],
                    "executable": false,
                    "lamports": 10000,
                    "owner": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                    "rent_epoch": 0,
                },
                "subscription": 1
            }
        });

        let (send, recv) = channel();

        send.send(Event::Message(ws::Message::Text(json.to_string())))
            .unwrap();

        assert_eq!(process_pubsub(recv.recv()), Some(10_000));
    }

    #[test]
    #[should_panic]
    fn test_pubsub_processor_bad_message() {
        let json = json!({
            "jsonrpc": "2.0",
            "method":"accountNotification",
            "params": {
                "result": {
                    "data": [],
                    "executable": false,
                    "owner": [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
                    "rent_epoch": 0,
                },
                "subscription": 1
            }
        });

        let (send, recv) = channel();

        send.send(Event::Message(ws::Message::Text(json.to_string())))
            .unwrap();

        assert_eq!(process_pubsub(recv.recv()), Some(10_000));
    }
}
