// Release builds on Windows are GUI apps, so no console window appears.
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

mod agent;
mod assets;
mod audio;
mod blocked;
mod bonjour;
mod clipboard;
mod console;
mod cursor;
mod devices;
mod input;
mod install;
mod lan;
mod lang;
mod pairing;
#[cfg(target_os = "macos")]
mod mac;
mod protocol;
mod quality;
mod relay;
mod request;
mod rtc;
mod settings;
mod sharer;
mod stop_key;
mod terminal;
mod util;
mod video;
mod window;
mod words;

use std::{
    net::{Ipv4Addr, SocketAddr, TcpListener as StdListener, TcpStream},
    process::exit,
    sync::Arc,
    time::Duration,
};

use agent::Agent;

const CONSOLE_PORT: u16 = 48231;
/// Port 80 keeps the help address free of a port number; the second is used when 80 is taken.
const LAN_PORTS: [u16; 2] = [80, 48232];

fn port(name: &str) -> Option<u16> {
    std::env::var(name).ok().and_then(|value| value.parse().ok())
}

/// Asks whatever holds the console port to show its window, and reports whether it was Light Assist
/// and did so. This is what makes a second launch open the app that is already running.
fn show_running_instance(console_port: u16) -> bool {
    use std::io::{Read, Write};

    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, console_port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));

    let request = format!("POST /api/window/show HTTP/1.1\r\nHost: 127.0.0.1:{console_port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let mut reply = String::new();
    stream.write_all(request.as_bytes()).is_ok()
        && stream.read_to_string(&mut reply).is_ok()
        && reply.starts_with("HTTP/1.1 200")
        && reply.contains(r#""app":"light-assist""#)
}

async fn serve(agent: Arc<Agent>, console: StdListener) {
    let console = tokio::net::TcpListener::from_std(console).expect("console listener");
    let console_app = console::router(agent.clone());
    // With unattended access on, trusted devices can connect from the moment the app is running.
    agent.stand_by().await;

    let expiry = async {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tick.tick().await;
            agent.expire();
        }
    };

    let _ = tokio::join!(async { axum::serve(console, console_app).await }, expiry);
}

fn main() {
    // Physical pixels everywhere: the screen capture, the monitors' positions and the mouse must all
    // agree, which they do not when Windows scales coordinates for the process on high-DPI screens.
    #[cfg(windows)]
    unsafe {
        use windows::Win32::UI::HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    // WinRT (the Store package's startup task) and COM (shortcuts) work on any thread from here on.
    #[cfg(windows)]
    unsafe {
        let _ = windows::Win32::System::Com::CoIncrementMTAUsage();
    }

    let arguments: Vec<String> = std::env::args().collect();
    if arguments.iter().any(|argument| argument == install::UNINSTALL_ARGUMENT) {
        install::uninstall();
        return;
    }

    let console_port = port("LIGHT_ASSIST_CONSOLE_PORT").unwrap_or(CONSOLE_PORT);
    let lan_ports = port("LIGHT_ASSIST_LAN_PORT").map_or(LAN_PORTS.to_vec(), |port| vec![port]);
    let headless = std::env::args().any(|arg| arg == "--headless") || std::env::var_os("LIGHT_ASSIST_NO_WINDOW").is_some();

    let address = format!("http://127.0.0.1:{console_port}");
    let bind = || StdListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, console_port)));
    // A freshly installed copy starts while the portable one is still letting go of the port.
    let mut bound = bind();
    if arguments.iter().any(|argument| argument == install::AFTER_INSTALL_ARGUMENT) {
        for _ in 0..50 {
            if bound.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
            bound = bind();
        }
    }
    let console = bound.unwrap_or_else(|error| {
        // The port is taken. If Light Assist is already running, starting it again just opens its window.
        if show_running_instance(console_port) {
            exit(0);
        }
        eprintln!("Console could not listen on 127.0.0.1:{console_port}: {error}");
        exit(1);
    });
    console.set_nonblocking(true).expect("could not make the listener non-blocking");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("could not start the async runtime");

    // The console's actions need a key that only the app's own window is given. Without it, a helper
    // with mouse and keyboard control could open the console in the sharer's browser and allow
    // themselves more. A fixed key can be set for development and tests.
    let key = std::env::var("LIGHT_ASSIST_CONSOLE_KEY").ok().filter(|key| !key.is_empty()).unwrap_or_else(|| {
        let random = [(); 4].map(|()| getrandom::u32().expect("OS randomness is unavailable"));
        random.iter().map(|part| format!("{part:08x}")).collect()
    });
    let console_page = format!("{address}/#key={key}");

    let agent = Agent::new(address.clone(), key.clone(), lan_ports);

    if headless {
        println!("Console (this device only): {console_page}");
        println!("Nothing listens on the LAN until a help request starts.");
        runtime.block_on(async {
            tokio::select! {
                _ = serve(agent.clone(), console) => {}
                _ = tokio::signal::ctrl_c() => {}
            }
        });
        agent.shutdown();
        return;
    }

    let server_agent = agent.clone();
    std::thread::spawn(move || runtime.block_on(serve(server_agent, console)));
    let opener_agent = agent.clone();
    window::run(address, key, install::started_at_login(), move |desktop| opener_agent.set_desktop(desktop), move || agent.shutdown());
}
