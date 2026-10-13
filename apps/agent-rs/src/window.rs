//! The app's presence on the desktop: a tray icon that is always there (in the menu bar, on a Mac),
//! the console window, which exists only while it is open, and the indicator shown on screen for as
//! long as a helper is connected.

use std::path::PathBuf;

use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget},
    monitor::MonitorHandle,
    window::{Icon as WindowIcon, Window, WindowBuilder, WindowId},
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    Icon as TrayImage, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use wry::{NewWindowResponse, WebContext, WebView, WebViewBuilder};

use crate::lang::tr;

const ICON_SIZE: u32 = 32;
/// Wide enough for every label at once: screen, mouse and keyboard, terminal, and the Stop button.
const INDICATOR_SIZE: (f64, f64) = (740.0, 44.0);
/// How far below the top of the screen the indicator first appears.
const INDICATOR_TOP: f64 = 10.0;

/// What the agent can ask of the desktop, from any thread.
pub struct Desktop {
    /// Brings the console window up.
    pub show_console: Box<dyn Fn() + Send + Sync>,
    /// Shows or removes the on-screen indicator that a helper is connected.
    pub set_sharing: Box<dyn Fn(bool) + Send + Sync>,
    /// Makes the indicator let the mouse through to what is beneath it, or take it again, and waits
    /// until it has: the helper does not see the indicator, and clicks where it is meaning what is under it.
    pub let_through_indicator: Box<dyn Fn(bool) + Send + Sync>,
    /// Ends the app, as Quit in the tray menu does.
    pub quit: Box<dyn Fn() + Send + Sync>,
}

#[derive(Clone, Copy)]
enum WindowAction {
    /// The page has drawn itself: the window can be shown without a blank moment.
    Ready,
    Drag,
    Minimize,
    Close,
}

enum AppEvent {
    Show,
    Quit,
    Sharing(bool),
    /// The indicator is to let the mouse through, or to take it again; answered when done.
    LetThrough(bool, std::sync::mpsc::SyncSender<()>),
    /// The page in one of the app's windows that draws its own title bar asks for what a title bar does.
    Window(WindowId, WindowAction),
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

fn data_directory() -> Option<PathBuf> {
    // Keeps WebView2 state out of the folder that contains the executable.
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("light-assist").join("webview"))
    } else {
        None
    }
}

/// The app's mark, four rounded squares leaning a little to the left as they do beside the app's
/// name, drawn here so that the binary carries no image files.
fn icon_pixels() -> Vec<u8> {
    const RED: [f64; 3] = [223.0, 99.0, 70.0];
    const GREEN: [f64; 3] = [141.0, 189.0, 119.0];
    /// Points looked at inside each pixel, along each side, so that the tilted edges come out smooth.
    const SAMPLES: u32 = 4;
    let tilt = (-8.0f64).to_radians();
    let (sin, cos) = tilt.sin_cos();
    let size = ICON_SIZE as f64;
    // A tilted square reaches further out than an upright one, so the mark is drawn that much smaller.
    let span = size * 0.94 / (cos - sin);
    let gap = span * 0.07;
    let square = (span - gap) / 2.0;
    let radius = square * 0.22;

    let mut pixels = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        let (x, y) = ((index as u32 % ICON_SIZE) as f64, (index as u32 / ICON_SIZE) as f64);
        let (mut red, mut green) = (0u32, 0u32);
        for sample in 0..SAMPLES * SAMPLES {
            let (within_x, within_y) = (((sample % SAMPLES) as f64 + 0.5) / SAMPLES as f64, ((sample / SAMPLES) as f64 + 0.5) / SAMPLES as f64);
            // Where this point of the picture is on the mark before it was tilted.
            let (dx, dy) = (x + within_x - size / 2.0, y + within_y - size / 2.0);
            let (px, py) = (cos * dx + sin * dy + span / 2.0, -sin * dx + cos * dy + span / 2.0);
            if !(0.0..span).contains(&px) || !(0.0..span).contains(&py) {
                continue;
            }
            let part = |position: f64| if position < square { Some((0, position)) } else if position >= square + gap { Some((1, position - square - gap)) } else { None };
            let (Some((column, inner_x)), Some((row, inner_y))) = (part(px), part(py)) else { continue };
            // How far into a corner the point is; beyond the corner's radius it is outside the rounding.
            let into = |position: f64| (radius - position).max(position - (square - radius)).max(0.0);
            if into(inner_x).hypot(into(inner_y)) <= radius {
                *(if column == row { &mut red } else { &mut green }) += 1;
            }
        }
        let covered = red + green;
        if covered == 0 {
            continue;
        }
        let color = if red >= green { RED } else { GREEN };
        for (channel, value) in pixel.iter_mut().zip(color) {
            *channel = value as u8;
        }
        pixel[3] = (covered * 255 / (SAMPLES * SAMPLES)) as u8;
    }
    pixels
}

/// Puts the app's own page at `url` into `window` (WebView2, WKWebView or WebKitGTK).
fn webview(window: &Window, context: &mut WebContext, home: &str, url: &str, on_message: impl Fn(&str) + 'static) -> WebView {
    let home = home.to_owned();
    let builder = WebViewBuilder::new_with_web_context(context)
        .with_url(url)
        // On a Mac the first click on a window that is not in front only brings it forward. The
        // indicator's buttons, Stop above all, have to work at once.
        .with_accept_first_mouse(true)
        .with_devtools(cfg!(debug_assertions))
        // The app's pages never need to leave their own origin.
        .with_navigation_handler(move |target| target == home || target.starts_with(&format!("{home}/")))
        .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
        .with_ipc_handler(move |request| on_message(request.body()));

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let webview = builder.build(window).expect("could not create the webview");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().expect("the window has no GTK container");
        builder.build_gtk(vbox).expect("could not create the webview")
    };
    webview
}

/// The console's width, which does not change: its page is laid out as one column of this width.
/// Only its height can be changed, for a longer or shorter view of the same column.
const CONSOLE_WIDTH: f64 = 440.0;

/// Whether the console draws its own title bar: on Windows, where the system's would add a second bar,
/// with its own icon and name, above the page's. The page then has the minimize and close buttons.
const OWN_TITLE_BAR: bool = cfg!(windows);

/// How long a new console window waits, hidden, for its page to say it has drawn itself, before it is
/// shown anyway.
const READY_WITHIN: std::time::Duration = std::time::Duration::from_millis(1500);

/// Creates the console window, hidden until its page has drawn itself, so that it never appears blank
/// first. The webview comes first in the pair so that it is dropped before the window it lives in.
fn open_console(target: &EventLoopWindowTarget<AppEvent>, context: &mut WebContext, home: &str, page: &str, proxy: EventLoopProxy<AppEvent>) -> (WebView, Window) {
    let builder = WindowBuilder::new()
        .with_title("Light Assist")
        // A small utility window, not a browser-sized page: taller or shorter, never wider, never full screen.
        .with_inner_size(LogicalSize::new(CONSOLE_WIDTH, 640.0))
        .with_min_inner_size(LogicalSize::new(CONSOLE_WIDTH, 520.0))
        .with_max_inner_size(LogicalSize::new(CONSOLE_WIDTH, 4000.0))
        .with_maximizable(false)
        .with_visible(false)
        .with_window_icon(WindowIcon::from_rgba(icon_pixels(), ICON_SIZE, ICON_SIZE).ok());
    #[cfg(windows)]
    let builder = {
        use tao::platform::windows::WindowBuilderExtWindows;
        // Without the system's frame, Windows still gives the window its shadow and lets its edges be dragged.
        builder.with_decorations(false).with_undecorated_shadow(true)
    };
    let window = builder.build(target).expect("could not create the window");

    let id = window.id();
    // A page that never says so (it failed to load, say) must not leave the window hidden for good.
    let late = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_WITHIN);
        let _ = late.send_event(AppEvent::Window(id, WindowAction::Ready));
    });
    let view = webview(&window, context, home, page, move |message| {
        let action = match message {
            "ready" => WindowAction::Ready,
            "drag" => WindowAction::Drag,
            "minimize" => WindowAction::Minimize,
            "close" => WindowAction::Close,
            _ => return,
        };
        let _ = proxy.send_event(AppEvent::Window(id, action));
    });
    (view, window)
}

/// Creates an indicator on `monitor`: a small bar that stays above every other window, so that the
/// sharer can always see that someone is connected, and what they can do, even with the console closed.
/// There is one on every screen, so it is seen wherever the sharer is looking.
fn open_indicator(
    target: &EventLoopWindowTarget<AppEvent>,
    context: &mut WebContext,
    home: &str,
    page: &str,
    proxy: EventLoopProxy<AppEvent>,
    monitor: Option<MonitorHandle>,
) -> Option<(WebView, Window)> {
    let builder = WindowBuilder::new()
        .with_title(tr("Light Assist is sharing this computer", "Light Assist sta condividendo questo computer", "Light Assist está compartiendo este ordenador"))
        .with_inner_size(LogicalSize::new(INDICATOR_SIZE.0, INDICATOR_SIZE.1))
        .with_decorations(false)
        .with_resizable(false)
        .with_always_on_top(true)
        // It must not take the keyboard away from whatever the sharer is doing.
        .with_focused(false)
        // On a Mac: also over a full-screen app, and whichever desktop the sharer switches to.
        .with_visible_on_all_workspaces(true);
    #[cfg(windows)]
    let builder = {
        use tao::platform::windows::WindowBuilderExtWindows;
        builder.with_skip_taskbar(true)
    };
    // Top center of its screen, where it is seen without covering much. It can be dragged elsewhere.
    let builder = match monitor {
        Some(monitor) => {
            let (scale, area, origin) = (monitor.scale_factor(), monitor.size(), monitor.position());
            let left = origin.x as f64 + (area.width as f64 - INDICATOR_SIZE.0 * scale) / 2.0;
            builder.with_position(PhysicalPosition::new(left, origin.y as f64 + INDICATOR_TOP * scale))
        }
        None => builder,
    };
    let window = builder.build(target).inspect_err(|error| eprintln!("No on-screen indicator: {error}")).ok()?;

    // The helper is shown the sharer's screen, not this app's own notice about it. On a Mac the screen
    // capture leaves it out, knowing it as the app's one window that floats above the others.
    // `LIGHT_ASSIST_SCREENSHOTS` leaves it in, for taking pictures of the app: Windows otherwise keeps
    // it out of every capture, the Snipping Tool's included.
    #[cfg(windows)]
    if std::env::var_os("LIGHT_ASSIST_SCREENSHOTS").is_none() {
        use tao::platform::windows::WindowExtWindows;
        use windows::Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE},
        };
        unsafe {
            let _ = SetWindowDisplayAffinity(HWND(window.hwnd() as _), WDA_EXCLUDEFROMCAPTURE);
        }
    }

    let id = window.id();
    let view = webview(&window, context, home, page, move |message| {
        if message == "drag" {
            let _ = proxy.send_event(AppEvent::Window(id, WindowAction::Drag));
        }
    });
    Some((view, window))
}

/// A Mac app's menus at the top of the screen. Without an Edit menu, its shortcuts for copying and
/// pasting do nothing in the console; and Quit is expected there, with its shortcut.
#[cfg(target_os = "macos")]
fn app_menu(quit: &MenuItem) -> Option<Menu> {
    use tray_icon::menu::{PredefinedMenuItem, Submenu};

    let app = Submenu::with_items("Light Assist", true, &[&PredefinedMenuItem::hide(None), &PredefinedMenuItem::separator(), quit]).ok()?;
    let edit = Submenu::with_items(
        tr("Edit", "Modifica", "Edición"),
        true,
        &[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::select_all(None),
        ],
    )
    .ok()?;
    let menu = Menu::with_items(&[&app, &edit]).ok()?;
    menu.init_for_nsapp();
    Some(menu)
}

/// Whether the app shows in the Dock and has menus: only while its window is open. The rest of the
/// time it is an icon in the menu bar, like the tray icon it is on Windows.
#[cfg(target_os = "macos")]
fn show_in_dock(target: &EventLoopWindowTarget<AppEvent>, shown: bool) {
    use tao::platform::macos::{ActivationPolicy, EventLoopWindowTargetExtMacOS};
    target.set_activation_policy_at_runtime(if shown { ActivationPolicy::Regular } else { ActivationPolicy::Accessory });
}

#[cfg(not(target_os = "macos"))]
fn show_in_dock(_target: &EventLoopWindowTarget<AppEvent>, _shown: bool) {}

/// Makes a window let the mouse through to what is beneath it, or take it again, before returning:
/// whoever asked is about to send a click and is waiting to be told the way is clear.
#[cfg(target_os = "macos")]
fn let_mouse_through(window: &Window, through: bool) {
    use objc2::{msg_send, runtime::AnyObject};
    use tao::platform::macos::WindowExtMacOS;

    // Asked of the window directly: the window library would only get round to it later, after
    // saying it was done, and the click would arrive first.
    let ns_window = window.ns_window() as *mut AnyObject;
    if !ns_window.is_null() {
        let _: () = unsafe { msg_send![&*ns_window, setIgnoresMouseEvents: through] };
        if std::env::var_os("LIGHT_ASSIST_TRACE").is_some() {
            let ignores: bool = unsafe { msg_send![&*ns_window, ignoresMouseEvents] };
            eprintln!("trace: the indicator was asked to let the mouse through: {through}; it now does: {ignores}");
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn let_mouse_through(window: &Window, through: bool) {
    let _ = window.set_ignore_cursor_events(through);
}

/// The mark as a Mac draws its menu bar icons: in one color, which the system chooses to suit the
/// menu bar, light or dark. Only how solid each pixel is counts; the green pair is drawn fainter, so
/// that the two pairs can still be told apart.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn monochrome(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.chunks_exact_mut(4) {
        let green = pixel[1] > pixel[0];
        pixel[3] = if green { (pixel[3] as u16 * 2 / 5) as u8 } else { pixel[3] };
        pixel[..3].fill(0);
    }
    pixels
}

fn tray(open: &MenuItem, quit: &MenuItem) -> Option<TrayIcon> {
    let menu = Menu::new();
    menu.append(open).ok()?;
    menu.append(quit).ok()?;

    let builder = TrayIconBuilder::new().with_tooltip("Light Assist");
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(TrayImage::from_rgba(monochrome(icon_pixels()), ICON_SIZE, ICON_SIZE).ok()?);
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(TrayImage::from_rgba(icon_pixels(), ICON_SIZE, ICON_SIZE).ok()?);
    builder
        .with_menu(Box::new(menu))
        // A left click opens the window; the menu is for the right button.
        .with_menu_on_left_click(false)
        .build()
        .inspect_err(|error| eprintln!("No tray icon: {error}"))
        .ok()
}

/// Runs the app's windows and tray icon. Never returns.
///
/// Closing the console leaves the app running in the tray, so a session carries on. The window and
/// its webview are destroyed, not hidden, because a hidden webview would keep several hundred
/// megabytes of browser processes alive for nothing.
/// With `start_hidden`, as when Windows starts the app at login, only the tray icon appears.
pub fn run(url: String, key: String, start_hidden: bool, share: impl FnOnce(Desktop), mut on_exit: impl FnMut() + 'static) -> ! {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    // Opened at login, the app is only its menu bar icon until its window is asked for.
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
        event_loop.set_activation_policy(if start_hidden { ActivationPolicy::Accessory } else { ActivationPolicy::Regular });
    }
    // The key travels in the part of the address that is never sent over the network.
    let title_bar = if OWN_TITLE_BAR { "&titlebar=own" } else { "" };
    let pages = (format!("{url}/#key={key}{title_bar}"), format!("{url}/indicator.html#key={key}"));

    let (shower, sharer, quitter, passer) = (event_loop.create_proxy(), event_loop.create_proxy(), event_loop.create_proxy(), event_loop.create_proxy());
    share(Desktop {
        show_console: Box::new(move || {
            let _ = shower.send_event(AppEvent::Show);
        }),
        set_sharing: Box::new(move |sharing| {
            let _ = sharer.send_event(AppEvent::Sharing(sharing));
        }),
        let_through_indicator: Box::new(move |through| {
            let (done, answered) = std::sync::mpsc::sync_channel(1);
            // The windows belong to the event loop's thread. If it is slow to answer, the click goes
            // ahead anyway and may land on the bar, which is no worse than it was.
            if passer.send_event(AppEvent::LetThrough(through, done)).is_ok() {
                let _ = answered.recv_timeout(std::time::Duration::from_millis(100));
            }

        }),
        quit: Box::new(move || {
            let _ = quitter.send_event(AppEvent::Quit);
        }),
    });
    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(AppEvent::Tray(event));
    }));
    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(AppEvent::Menu(event));
    }));
    let proxy = event_loop.create_proxy();

    let open_item = MenuItem::new(tr("Open Light Assist", "Apri Light Assist", "Abrir Light Assist"), true, None);
    let quit_item = MenuItem::new(tr("Quit", "Esci", "Salir"), true, None);
    // The same Quit as the tray's, so that it says goodbye on the network too.
    #[cfg(target_os = "macos")]
    let app_quit_item = {
        use tray_icon::menu::accelerator::{Accelerator, Code, Modifiers};
        MenuItem::new(tr("Quit Light Assist", "Esci da Light Assist", "Salir de Light Assist"), true, Some(Accelerator::new(Modifiers::META, Code::KeyQ)))
    };
    #[cfg(target_os = "macos")]
    let mut app_menu_bar: Option<Menu> = None;
    let mut context = WebContext::new(data_directory());
    let mut console: Option<(WebView, Window)> = None;
    let mut indicators: Vec<(WebView, Window)> = Vec::new();
    let mut tray_icon: Option<TrayIcon> = None;

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;

        let (show, quit) = match event {
            // The tray icon can only be created once the event loop is running.
            Event::NewEvents(StartCause::Init) => {
                tray_icon = tray(&open_item, &quit_item);
                #[cfg(target_os = "macos")]
                {
                    app_menu_bar = app_menu(&app_quit_item);
                }
                // Hidden only if there is a tray icon to come back through.
                (!start_hidden || tray_icon.is_none(), false)
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, window_id, .. } => {
                if console.as_ref().is_some_and(|(_, window)| window.id() == window_id) {
                    console = None;
                    show_in_dock(target, tray_icon.is_none());
                }
                // Without a tray icon there would be no way back to the window, so closing it quits.
                (false, console.is_none() && tray_icon.is_none())
            }
            Event::UserEvent(AppEvent::Show) => (true, false),
            // A click on the app's icon in the Dock.
            #[cfg(target_os = "macos")]
            Event::Reopen { .. } => (true, false),
            Event::UserEvent(AppEvent::Quit) => (false, true),
            Event::UserEvent(AppEvent::Sharing(sharing)) => {
                if !sharing {
                    indicators.clear();
                } else if indicators.is_empty() {
                    let mut monitors: Vec<Option<MonitorHandle>> = target.available_monitors().map(Some).collect();
                    if monitors.is_empty() {
                        monitors.push(None);
                    }
                    indicators = monitors
                        .into_iter()
                        .filter_map(|monitor| open_indicator(target, &mut context, &url, &pages.1, proxy.clone(), monitor))
                        .collect();
                }
                if let Some(tray_icon) = &tray_icon {
                    let _ = tray_icon.set_tooltip(Some(if sharing { tr("Light Assist: an assistant is connected", "Light Assist: un assistente è connesso", "Light Assist: hay un asistente conectado") } else { "Light Assist" }));
                }
                (false, false)
            }
            Event::UserEvent(AppEvent::LetThrough(through, done)) => {
                for (_, window) in &indicators {
                    let_mouse_through(window, through);
                }
                let _ = done.try_send(());
                (false, false)
            }
            Event::UserEvent(AppEvent::Window(id, action)) => {
                let console_is = console.as_ref().is_some_and(|(_, window)| window.id() == id);
                match action {
                    WindowAction::Close if console_is => {
                        // As the system's own close button does: the app stays in the tray.
                        console = None;
                        show_in_dock(target, tray_icon.is_none());
                        (false, tray_icon.is_none())
                    }
                    WindowAction::Close => (false, false),
                    WindowAction::Minimize => {
                        if let Some((_, window)) = console.as_ref().filter(|_| console_is) {
                            window.set_minimized(true);
                        }
                        (false, false)
                    }
                    WindowAction::Ready => {
                        if let Some((_, window)) = console.as_ref().filter(|(_, window)| console_is && !window.is_visible()) {
                            window.set_visible(true);
                            window.set_focus();
                        }
                        (false, false)
                    }
                    WindowAction::Drag => {
                        let window = indicators.iter().chain(console.as_ref()).map(|(_, window)| window).find(|window| window.id() == id);
                        if let Some(window) = window {
                            let _ = window.drag_window();
                        }
                        (false, false)
                    }
                }
            }
            Event::UserEvent(AppEvent::Tray(TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            })) => (true, false),
            #[cfg(target_os = "macos")]
            Event::UserEvent(AppEvent::Menu(event)) => (event.id() == open_item.id(), event.id() == quit_item.id() || event.id() == app_quit_item.id()),
            #[cfg(not(target_os = "macos"))]
            Event::UserEvent(AppEvent::Menu(event)) => (event.id() == open_item.id(), event.id() == quit_item.id()),
            _ => (false, false),
        };

        if show {
            show_in_dock(target, true);
            let (_, window) = console.get_or_insert_with(|| open_console(target, &mut context, &url, &pages.0, proxy.clone()));
            // A new window shows itself once its page is ready; one already there comes to the front.
            if window.is_visible() {
                window.set_minimized(false);
                window.set_focus();
            }
        }
        if quit {
            console = None;
            indicators.clear();
            tray_icon = None;
            #[cfg(target_os = "macos")]
            drop(app_menu_bar.take());
            on_exit();
            *control_flow = ControlFlow::Exit;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tray icon has both colors, see-through corners, and soft edges from being tilted.
    #[test]
    fn the_mark_is_drawn_tilted_and_smooth() {
        let pixels = icon_pixels();
        let count = |test: fn(&[u8]) -> bool| pixels.chunks_exact(4).filter(|pixel| test(pixel)).count();
        assert!(count(|pixel| pixel[3] == 255 && pixel[0] > pixel[1]) > 100, "red squares");
        assert!(count(|pixel| pixel[3] == 255 && pixel[1] > pixel[0]) > 100, "green squares");
        assert!(count(|pixel| pixel[3] > 0 && pixel[3] < 255) > 40, "softened edges");
        assert_eq!(pixels[3], 0, "an empty corner");
    }

    /// On a Mac the menu bar icon is one color, with the green pair fainter than the red.
    #[test]
    fn the_menu_bar_icon_has_one_color_and_two_strengths() {
        let pixels = monochrome(icon_pixels());
        assert!(pixels.chunks_exact(4).all(|pixel| pixel[..3] == [0, 0, 0]));
        let solid = pixels.chunks_exact(4).filter(|pixel| pixel[3] == 255).count();
        let faint = pixels.chunks_exact(4).filter(|pixel| pixel[3] == 102).count();
        assert!(solid > 100 && faint > 100, "{solid} solid and {faint} faint pixels");
    }
}
